// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the config (tests/cpu/Config_tests.cpp @ v2.5.2) that read, write and validate
//! configs without building processors (WP 3.3, 3.7, 3.8), and the two `Config` tests of
//! tests/cpu/Display_tests.cpp. The tests that build processors come with the builders (WP
//! 3.2d).

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::test_env::EnvGuard;
use crate::transforms::exponent_transform::ExponentTransform;
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::transforms::range_transform::RangeTransform;
use ocio_ops::parse_utils::ROLE_COMPOSITING_LOG;
use ocio_ops::utils::string_utils::split_by_lines;

/// `setMajorVersion` takes 1 and 2 and refuses other versions (Config_tests.cpp:2065-2067,
/// 2104-2107 @ v2.5.2); the minor version becomes the last one each major supports, which
/// upstream's tests read back as the serialized profile version (Config_tests.cpp:2069-2088).
#[test]
fn set_major_version() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();

    config.set_major_version(1).unwrap();
    assert_eq!(config.major_version(), 1);
    check_throw_what(
        config.set_major_version(20000),
        "version is 20000 where supported versions start at 1 and end at 2",
    );
    assert_eq!(config.major_version(), 1);
    check_throw_what(
        config.set_major_version(3),
        "version is 3 where supported versions start at 1 and end at 2",
    );
    check_throw_what(
        config.set_major_version(0),
        "where supported versions start at 1 and end at 2",
    );

    config.set_major_version(2).unwrap();
    assert_eq!(config.major_version(), 2);
}

/// Port of `OCIO_ADD_TEST(Config, family_separator)` @ v2.5.2.
#[test]
fn family_separator() {
    let _env = EnvGuard::new();
    // Test the family separator.

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    cfg.validate().unwrap();

    assert_eq!(cfg.family_separator(), b'/');

    cfg.set_family_separator(b' ').unwrap();
    assert_eq!(cfg.family_separator(), b' ');

    cfg.set_family_separator(0).unwrap();
    assert_eq!(cfg.family_separator(), 0);

    // Reset to its default value.
    assert_eq!(Config::default_family_separator(), b'/');
    cfg.set_family_separator(Config::default_family_separator())
        .unwrap();
    assert_eq!(cfg.family_separator(), b'/');

    assert!(cfg.set_family_separator(127).is_err());
    assert!(cfg.set_family_separator(31).is_err());

    // Test read/write.

    const CONFIG: &str = concat!(
        "ocio_profile_version: 2\n",
        "\n",
        "environment:\n",
        "  {}\n",
        "search_path: \"\"\n",
        "strictparsing: false\n",
        "family_separator: \" \"\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "\n",
        "roles:\n",
        "  default: raw\n",
        "\n",
        "file_rules:\n",
        "  - !<Rule> {name: Default, colorspace: default}\n",
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
        "    family: raw\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: 32f\n",
        "    description: A raw color space. Conversions to and from this space are no-ops.\n",
        "    isdata: true\n",
        "    allocation: uniform\n",
    );

    cfg.set_family_separator(b' ').unwrap();

    check_serialized(&cfg.serialize().unwrap(), CONFIG);

    // v1 does not support family separators different from the default value i.e. '/'.

    const CONFIG_V1: &str = concat!(
        "ocio_profile_version: 1\n",
        "\n",
        "search_path: \"\"\n",
        "\n",
        "roles:\n",
        "  reference: raw\n",
        "\n",
        "displays:\n",
        "  sRGB:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: raw\n",
        "    allocation: uniform\n",
    );

    let mut cfg = (*Config::create_from_stream(CONFIG_V1.as_bytes()).unwrap()).clone();
    assert_eq!(cfg.family_separator(), b'/'); // v1 default family separator

    cfg.set_family_separator(b'&').unwrap();
    check_throw_what(
        cfg.validate(),
        "Only version 2 (or higher) can have a family separator.",
    );

    check_throw_what(
        cfg.serialize(),
        "Only version 2 (or higher) can have a family separator.",
    );

    // Even with the default value, v1 config file must not contain the family_separator key.

    const CONFIG_V1BIS: &str = concat!(
        "ocio_profile_version: 1\n",
        "\n",
        "search_path: \"\"\n",
        "family_separator: \"/\"\n",
        "\n",
        "roles:\n",
        "  reference: raw\n",
        "\n",
        "displays:\n",
        "  sRGB:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: raw\n",
        "    allocation: uniform\n",
    );

    check_throw_what(
        Config::create_from_stream(CONFIG_V1BIS.as_bytes()),
        "Config v1 can't have 'family_separator'.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, alias_validation)` @ v2.5.2.
#[test]
fn alias_validation() {
    let _env = EnvGuard::new();
    // NB: This tests ColorSpaceSet::addColorSpace.

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::new();
    cs.set_name("colorspace1");
    cfg.add_color_space(&cs).unwrap();
    cs.set_name("colorspace2");
    cfg.add_color_space(&cs).unwrap();
    cfg.validate().unwrap();
    cs.set_name("colorspace3");
    cs.add_alias("colorspace1");
    check_throw_what(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has 'colorspace1' alias and existing color \
         space, 'colorspace1' is using the same alias",
    );
    cs.remove_alias("colorspace1");

    cfg.set_role("alias", Some(b"colorspace2")).unwrap();
    cs.add_alias("alias");
    check_throw_what(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has an alias 'alias' and there is already a \
         role with this name",
    );
    cs.remove_alias("alias");
    cs.add_alias("test%test");
    check_throw_what(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has an alias 'test%test' that cannot contain \
         a context variable reserved token i.e. % or $",
    );

    cs.remove_alias("test%test");
    cs.add_alias("namedtransform");
    cfg.add_color_space(&cs).unwrap();
    let mut nt = NamedTransform::new();
    nt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        TransformDirection::Forward,
    )
    .unwrap();
    nt.set_name("namedtransform");
    check_throw_what(
        cfg.add_named_transform(&nt),
        "Cannot add 'namedtransform' named transform, there is already a color space using this name as a name or as an alias: 'colorspace3",
    );

    nt.set_name("nt");
    cfg.add_named_transform(&nt).unwrap();
    cfg.validate().unwrap();

    nt.add_alias("namedtransform");
    check_throw_what(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'namedtransform' and there is already a color space using this name as a name or as an alias: 'colorspace3'",
    );

    nt.remove_alias("namedtransform");
    nt.add_alias("colorspace3");
    check_throw_what(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'colorspace3' and there is already a color space using this name as a name or as an alias: 'colorspace3'",
    );

    nt.remove_alias("colorspace3");
    nt.add_alias("alias");
    check_throw_what(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'alias' and there is already a role with this name",
    );

    nt.remove_alias("alias");
    nt.add_alias("test%test");
    check_throw_what(
        cfg.add_named_transform(&nt),
        "Cannot add 'nt' named transform, it has an alias 'test%test' that cannot contain a context variable reserved token i.e. % or $",
    );
}

/// The checks of `Config compare_displays` (Display_tests.cpp:335-514 @ v2.5.2) on its two
/// configs.
fn check_compare_displays(config1: Config, config2: Config) {
    {
        // Active (display, view) pair where the view is display-defined.
        assert_eq!(1, config1.num_displays());
        assert_eq!(b"sRGB", config1.default_display());

        assert_eq!(
            2,
            config1.num_views_of_type(ViewType::DisplayDefined, "sRGB")
        );
        assert_eq!(
            b"Raw",
            config1.view_of_type(ViewType::DisplayDefined, "sRGB", 0)
        );
        assert_eq!(
            b"view",
            config1.view_of_type(ViewType::DisplayDefined, "sRGB", 1)
        );

        assert!(!config1.is_view_shared("sRGB", "view"));

        assert_eq!(2, config1.num_views("sRGB"));
        assert_eq!(b"view", config1.view("sRGB", 0));

        // Inactive (display, view) pair where the view is a reference to a shared view.
        assert_eq!(1, config2.num_displays());
        assert_eq!(b"Raw", config2.default_display());
        assert_eq!(1, config2.num_views("Raw"));
        assert_eq!(b"Raw", config2.default_view("Raw"));

        assert_eq!(config2.num_displays_all(), 2);
        assert_eq!(config2.display_all(1), b"sRGB");

        assert_eq!(2, config2.num_views_of_type(ViewType::Shared, "sRGB"));
        assert_eq!(b"view", config2.view_of_type(ViewType::Shared, "sRGB", 0));
        assert_eq!(b"sview1", config2.view_of_type(ViewType::Shared, "sRGB", 1));

        assert!(config2.is_view_shared("sRGB", "view"));

        assert!(Config::are_views_equal(&config1, &config2, "sRGB", "view"));
    }

    {
        // Inactive (display, view) pair where the view is display-defined.
        assert_eq!(1, config1.num_displays());
        assert_eq!(b"sRGB", config1.default_display());
        assert_eq!(config1.display_all(0), b"Raw");
        assert_eq!(
            1,
            config1.num_views_of_type(ViewType::DisplayDefined, "Raw")
        );
        assert_eq!(
            b"Raw",
            config1.view_of_type(ViewType::DisplayDefined, "Raw", 0)
        );
        assert!(!config1.is_view_shared("Raw", "Raw"));

        // Active (display, view) pair where the view is display-defined.
        assert_eq!(1, config2.num_displays());
        assert_eq!(b"Raw", config2.default_display());
        assert_eq!(
            1,
            config2.num_views_of_type(ViewType::DisplayDefined, "Raw")
        );
        assert_eq!(
            b"Raw",
            config2.view_of_type(ViewType::DisplayDefined, "Raw", 0)
        );
        assert!(!config2.is_view_shared("Raw", "Raw"));

        assert!(Config::are_views_equal(&config1, &config2, "Raw", "Raw"));
    }

    {
        // Active (display, view) pair where the view is a reference to a shared view.
        assert_eq!(1, config1.num_views_of_type(ViewType::Shared, "sRGB"));
        assert_eq!(b"sview1", config1.view_of_type(ViewType::Shared, "sRGB", 0));
        assert!(config1.is_view_shared("sRGB", "sview1"));

        // Inactive (display, view) pair where the view is a reference to a shared view.
        assert_eq!(2, config2.num_views_of_type(ViewType::Shared, "sRGB"));
        assert_eq!(b"view", config2.view_of_type(ViewType::Shared, "sRGB", 0));
        assert_eq!(b"sview1", config2.view_of_type(ViewType::Shared, "sRGB", 1));
        assert!(config2.is_view_shared("sRGB", "sview1"));

        assert!(Config::are_views_equal(
            &config1, &config2, "sRGB", "sview1"
        ));
    }

    {
        let mut cfg1 = config1.clone();
        assert!(cfg1.has_view("sRGB", "Raw"));
        assert!(cfg1.has_view("sRGB", "view"));
        assert!(cfg1.has_view("sRGB", "sview1"));
        assert!(cfg1.has_view("Raw", "Raw"));

        cfg1.set_active_displays("Raw").unwrap();
        assert_eq!(1, cfg1.num_displays());
        assert_eq!(b"Raw", cfg1.default_display());

        assert!(cfg1.has_view("sRGB", "sview1"));

        cfg1.set_active_views("Raw").unwrap();
        assert_eq!(cfg1.num_views("sRGB"), 1);
        assert_eq!(cfg1.view("sRGB", 0), b"Raw");

        assert!(cfg1.has_view("sRGB", "Raw"));
        assert!(cfg1.has_view("sRGB", "sview1"));

        cfg1.set_active_displays("sRGB").unwrap();
        assert_eq!(1, cfg1.num_displays());
        assert_eq!(b"sRGB", cfg1.default_display());

        assert!(cfg1.has_view("sRGB", "sview1"));
    }

    {
        // Test when a display exists, but a view does not exist.

        let mut cfg1 = config1.clone();

        assert_eq!(b"sRGB", cfg1.default_display());
        assert_eq!(2, cfg1.num_views_of_type(ViewType::DisplayDefined, "sRGB"));
        assert_eq!(
            b"Raw",
            cfg1.view_of_type(ViewType::DisplayDefined, "sRGB", 0)
        );
        assert_eq!(
            b"view",
            cfg1.view_of_type(ViewType::DisplayDefined, "sRGB", 1)
        );

        assert!(cfg1.has_view("sRGB", "Raw"));
        assert!(Config::are_views_equal(&config1, &cfg1, "sRGB", "Raw"));

        // Remove the view from the display.
        cfg1.remove_display_view("sRGB", "Raw").unwrap();
        assert_eq!(1, cfg1.num_views_of_type(ViewType::DisplayDefined, "sRGB"));
        assert_eq!(
            b"view",
            cfg1.view_of_type(ViewType::DisplayDefined, "sRGB", 0)
        );

        assert!(!cfg1.has_view("sRGB", "Raw"));
        assert!(!Config::are_views_equal(&config1, &cfg1, "sRGB", "Raw"));
    }

    {
        // Test when a view exists, but a display does not exist.

        let mut cfg2 = config2.clone();

        assert_eq!(b"Raw", cfg2.default_display());
        assert_eq!(1, cfg2.num_views("Raw"));
        assert_eq!(b"Raw", cfg2.view("Raw", 0));
        assert_eq!(b"Raw", cfg2.active_views().as_slice());

        assert!(cfg2.has_view("Raw", "Raw"));
        assert!(Config::are_views_equal(&config2, &cfg2, "Raw", "Raw"));

        // Remove the view from the display and the display itself since it only has no more
        // views.
        assert_eq!(2, cfg2.num_displays_all());
        cfg2.remove_display_view("Raw", "Raw").unwrap();
        assert_eq!(1, cfg2.num_displays_all());

        // The view is still active.
        assert_eq!(b"Raw", cfg2.active_views().as_slice());

        assert!(!cfg2.has_view("Raw", "Raw"));
        assert!(!Config::are_views_equal(&config2, &cfg2, "Raw", "Raw"));
    }

    {
        // Test access of config-level shared views for hasView method.

        let mut cfg1 = config1.clone();

        assert_eq!(1, cfg1.num_views_of_type(ViewType::Shared, "sRGB"));
        assert_eq!(b"sview1", cfg1.view_of_type(ViewType::Shared, "sRGB", 0));
        assert!(cfg1.is_view_shared("sRGB", "sview1"));

        assert!(cfg1.has_view("sRGB", "sview1"));

        // Remove the shared view from the display.
        cfg1.remove_display_view("sRGB", "sview1").unwrap();
        assert_eq!(b"sRGB", config1.default_display());
        assert_eq!(0, cfg1.num_views_of_type(ViewType::Shared, "sRGB"));

        // Shared view still exists in the config.
        assert_eq!(1, cfg1.num_views_of_type(ViewType::Shared, ""));
        assert_eq!(b"sview1", cfg1.view_of_type(ViewType::Shared, "", 0));
        assert!(cfg1.is_view_shared("", "sview1"));

        assert!(!cfg1.has_view("sRGB", "sview1"));

        // When display name is null, hasView will only check config level shared views.
        assert!(cfg1.has_view("", "sview1"));
    }
}

/// Upgrading a version 1 config without a scene color space for its default file rule is an
/// error, where upstream's `noexcept` function ends the program (U-55); the config is left as
/// it was.
#[test]
fn upgrade_without_a_scene_color_space_is_refused() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs.set_name("display_only");
    config.add_color_space(&cs).unwrap();
    config.set_inactive_color_spaces("display_only");
    config.set_major_version(1).unwrap();
    let rules = config.file_rules().get().to_bytes();

    assert!(config.upgrade_to_latest_version().is_err());
    assert_eq!(config.major_version(), 1);
    assert_eq!(config.file_rules().get().to_bytes(), rules);
}

/// U-55's other trigger: the default rule's color space goes to the rule at index 1
/// (I-138), which the path search rule refuses; upstream's `noexcept` upgrade then ends the
/// program (the wheel's process exits with 127). The port returns the path search rule's error
/// and leaves the config as it was.
#[test]
fn upgrade_with_the_path_search_rule_at_index_1_is_refused() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    cs.set_name("a");
    config.add_color_space(&cs).unwrap();
    let mut rules = FileRules::new();
    rules.insert_rule(0, "g", "a", "*", "x").unwrap();
    rules.insert_path_search_rule(1).unwrap();
    config.set_file_rules(&rules);
    config.set_major_version(1).unwrap();
    let before = config.file_rules().get().to_bytes();

    check_throw_what(
        config.upgrade_to_latest_version(),
        "File rules: ColorSpaceNamePathSearch rule does not accept any color space.",
    );
    assert_eq!(config.major_version(), 1);
    assert_eq!(config.file_rules().get().to_bytes(), before);
}

// The profiles of upstream's tests read as YAML (tests/cpu/Config_tests.cpp:2192-2313, 6890 @
// v2.5.2).

const PROFILE_V2: &str = "ocio_profile_version: 2\n\
\n\
environment:\n  \
{}\n";

const SIMPLE_PROFILE_A: &str = "search_path: luts\n\
strictparsing: true\n\
luma: [0.2126, 0.7152, 0.0722]\n\
\n\
roles:\n  \
default: raw\n  \
scene_linear: lnh\n\
\n";

const SIMPLE_PROFILE_DISPLAYS_LOOKS: &str = "displays:\n  \
sRGB:\n    \
- !<View> {name: RawView, colorspace: raw}\n    \
- !<View> {name: LnhView, colorspace: lnh, looks: beauty}\n\
\n\
active_displays: []\n\
active_views: []\n\
\n\
looks:\n  \
- !<Look>\n    \
name: beauty\n    \
process_space: lnh\n    \
transform: !<CDLTransform> {slope: [1, 2, 1]}\n\
\n";

const SIMPLE_PROFILE_CS_V2: &str = "\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n\
\n  \
- !<ColorSpace>\n    \
name: log\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n    \
from_scene_reference: !<LogTransform> {base: 10}\n\
\n  \
- !<ColorSpace>\n    \
name: lnh\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n";

const DEFAULT_RULES: &str = "file_rules:\n  \
- !<Rule> {name: Default, colorspace: default}\n\
\n";

/// `PROFILE_V2_START` (Config_tests.cpp:2311-2312 @ v2.5.2).
fn profile_v2_start() -> String {
    [
        PROFILE_V2,
        SIMPLE_PROFILE_A,
        DEFAULT_RULES,
        SIMPLE_PROFILE_DISPLAYS_LOOKS,
        SIMPLE_PROFILE_CS_V2,
    ]
    .concat()
}

/// `PROFILE_V2_DCS_START` (Config_tests.cpp:6890-6891 @ v2.5.2).
fn profile_v2_dcs_start() -> String {
    [
        PROFILE_V2,
        SIMPLE_PROFILE_A,
        DEFAULT_RULES,
        SIMPLE_PROFILE_DISPLAYS_LOOKS,
    ]
    .concat()
}

/// Port of `OCIO_ADD_TEST(Config, colorspace_duplicate)` @ v2.5.2.
#[test]
fn colorspace_duplicate() {
    let _env = EnvGuard::new();
    const SIMPLE_PROFILE: &str = "ocio_profile_version: 2\n\
search_path: luts\n\
roles:\n  \
default: raw\n\
file_rules:\n  \
- !<Rule> {name: Default, colorspace: default}\n\
displays:\n  \
Disp1:\n    \
- !<View> {name: View1, colorspace: raw}\n\
active_displays: []\n\
active_views: []\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw_duplicated\n    \
name: raw\n\
\n";

    check_throw_what(
        Config::create_from_stream(SIMPLE_PROFILE.as_bytes()),
        "Key-value pair with key 'name' specified more than once. ",
    );
}

/// Port of `OCIO_ADD_TEST(Config, cdltransform_duplicate)` @ v2.5.2.
#[test]
fn cdltransform_duplicate() {
    let _env = EnvGuard::new();
    const SIMPLE_PROFILE: &str = "ocio_profile_version: 2\n\
search_path: luts\n\
roles:\n  \
default: raw\n\
file_rules:\n  \
- !<Rule> {name: Default, colorspace: default}\n\
displays:\n  \
Disp1:\n    \
- !<View> {name: View1, colorspace: raw}\n\
active_displays: []\n\
active_views: []\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n    \
to_scene_reference: !<CDLTransform> {slope: [1, 2, 1], slope: [1, 2, 1]}\n\
\n";

    check_throw_what(
        Config::create_from_stream(SIMPLE_PROFILE.as_bytes()),
        "Key-value pair with key 'slope' specified more than once. ",
    );
}

/// Port of `OCIO_ADD_TEST(Config, searchpath_duplicate)` @ v2.5.2.
#[test]
fn searchpath_duplicate() {
    let _env = EnvGuard::new();
    const SIMPLE_PROFILE: &str = "ocio_profile_version: 2\n\
search_path: luts\n\
search_path: luts-dir\n\
roles:\n  \
default: raw\n\
file_rules:\n  \
- !<Rule> {name: Default, colorspace: default}\n\
displays:\n  \
Disp1:\n    \
- !<View> {name: View1, colorspace: raw}\n\
active_displays: []\n\
active_views: []\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n\
\n";

    check_throw_what(
        Config::create_from_stream(SIMPLE_PROFILE.as_bytes()),
        "Key-value pair with key 'search_path' specified more than once. ",
    );
}

/// Port of `OCIO_ADD_TEST(Config, roles)` @ v2.5.2.
#[test]
fn roles() {
    let _env = EnvGuard::new();
    const SIMPLE_PROFILE: &str = "ocio_profile_version: 1\n\
strictparsing: false\n\
roles:\n  \
compositing_log: lgh\n  \
default: raw\n  \
scene_linear: lnh\n\
colorspaces:\n  \
- !<ColorSpace>\n      \
name: raw\n  \
- !<ColorSpace>\n      \
name: lnh\n  \
- !<ColorSpace>\n      \
name: lgh\n\
\n";

    let config = Config::create_from_stream(SIMPLE_PROFILE.as_bytes()).unwrap();

    assert_eq!(config.num_roles(), 3);

    assert!(config.has_role("compositing_log"));
    assert!(!config.has_role("cheese"));
    assert!(!config.has_role(""));

    assert_eq!(config.role_name(2), b"scene_linear");
    assert_eq!(config.role_color_space_by_index(2), b"lnh");

    assert_eq!(config.role_name(0), b"compositing_log");
    assert_eq!(config.role_color_space_by_index(0), b"lgh");

    assert_eq!(config.role_name(1), b"default");

    assert_eq!(config.role_name(10), b"");
    assert_eq!(config.role_color_space_by_index(10), b"");

    assert_eq!(config.role_name(-4), b"");
    assert_eq!(config.role_color_space_by_index(-4), b"");

    // Test existing roles.
    assert_eq!(config.role_color_space("scene_linear"), b"lnh");
    assert_eq!(config.role_color_space("compositing_log"), b"lgh");

    // Test a unknown role.
    assert_eq!(config.role_color_space("wrong_role"), b"");

    // Test an empty input.
    assert_eq!(config.role_color_space(""), b"");
}

/// Port of `OCIO_ADD_TEST(Config, view)` @ v2.5.2.
#[test]
fn view() {
    const SIMPLE_PROFILE_HEADER: &str = "ocio_profile_version: 1\n\
\n\
search_path: luts\n\
strictparsing: true\n\
luma: [0.2126, 0.7152, 0.0722]\n\
\n\
roles:\n  \
default: raw\n  \
scene_linear: lnh\n\
\n\
displays:\n  \
sRGB_1:\n    \
- !<View> {name: View_1, colorspace: raw}\n    \
- !<View> {name: View_2, colorspace: raw}\n  \
sRGB_2:\n    \
- !<View> {name: View_2, colorspace: raw}\n    \
- !<View> {name: View_3, colorspace: raw}\n  \
sRGB_3:\n    \
- !<View> {name: View_3, colorspace: raw}\n    \
- !<View> {name: View_1, colorspace: raw}\n\
\n";

    const SIMPLE_PROFILE_FOOTER: &str = "\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n\
\n  \
- !<ColorSpace>\n    \
name: lnh\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n";

    let profile = |active: &str| [SIMPLE_PROFILE_HEADER, active, SIMPLE_PROFILE_FOOTER].concat();

    let env = EnvGuard::new();
    {
        let config = Config::create_from_stream(
            profile("active_displays: []\nactive_views: []\n").as_bytes(),
        )
        .unwrap();
        assert_eq!(config.default_view("sRGB_1"), b"View_1");
        assert_eq!(config.num_views("sRGB_1"), 2);
        assert_eq!(config.view("sRGB_1", 0), b"View_1");
        assert_eq!(config.view("sRGB_1", 1), b"View_2");
        // Invalid index.
        assert_eq!(config.view("sRGB_1", 42), b"");

        assert_eq!(config.default_view("sRGB_2"), b"View_2");
        assert_eq!(config.num_views("sRGB_2"), 2);
        assert_eq!(config.view("sRGB_2", 0), b"View_2");
        assert_eq!(config.view("sRGB_2", 1), b"View_3");
        assert_eq!(config.default_view("sRGB_3"), b"View_3");
        assert_eq!(config.num_views("sRGB_3"), 2);
        assert_eq!(config.view("sRGB_3", 0), b"View_3");
        assert_eq!(config.view("sRGB_3", 1), b"View_1");

        assert_eq!(
            config.serialize().unwrap(),
            profile("active_displays: []\nactive_views: []\n").as_bytes()
        );
    }

    {
        let config = Config::create_from_stream(
            profile("active_displays: []\nactive_views: [View_3]\n").as_bytes(),
        )
        .unwrap();
        assert_eq!(config.default_view("sRGB_1"), b"View_1");
        // The active views list is ignored, for a display, if it would remove all views.
        assert_eq!(config.num_views("sRGB_1"), 2);
        assert_eq!(config.view("sRGB_1", 0), b"View_1");
        assert_eq!(config.view("sRGB_1", 1), b"View_2");
        assert_eq!(config.default_view("sRGB_2"), b"View_3");
        assert_eq!(config.num_views("sRGB_2"), 1);
        assert_eq!(config.view("sRGB_2", 0), b"View_3");
        assert_eq!(config.default_view("sRGB_3"), b"View_3");
        assert_eq!(config.num_views("sRGB_3"), 1);
        assert_eq!(config.view("sRGB_3", 0), b"View_3");

        assert_eq!(
            config.num_views_of_type(ViewType::DisplayDefined, "sRGB_1"),
            2
        );
        assert_eq!(
            config.num_views_of_type(ViewType::DisplayDefined, "sRGB_2"),
            2
        );
        assert_eq!(
            config.num_views_of_type(ViewType::DisplayDefined, "sRGB_3"),
            2
        );

        // Test that all views are saved.
        assert_eq!(
            config.serialize().unwrap(),
            profile("active_displays: []\nactive_views: [View_3]\n").as_bytes()
        );
    }

    {
        let config = Config::create_from_stream(
            profile("active_displays: []\nactive_views: [View_3, View_2, View_1]\n").as_bytes(),
        )
        .unwrap();
        assert_eq!(config.default_view("sRGB_1"), b"View_2");
        assert_eq!(config.num_views("sRGB_1"), 2);
        assert_eq!(config.view("sRGB_1", 0), b"View_2");
        assert_eq!(config.view("sRGB_1", 1), b"View_1");
        assert_eq!(config.default_view("sRGB_2"), b"View_3");
        assert_eq!(config.num_views("sRGB_2"), 2);
        assert_eq!(config.view("sRGB_2", 0), b"View_3");
        assert_eq!(config.view("sRGB_2", 1), b"View_2");
        assert_eq!(config.default_view("sRGB_3"), b"View_3");
        assert_eq!(config.num_views("sRGB_3"), 2);
        assert_eq!(config.view("sRGB_3", 0), b"View_3");
        assert_eq!(config.view("sRGB_3", 1), b"View_1");
    }

    {
        env.set(&[("OCIO_ACTIVE_VIEWS", " View_3, View_2")]);
        let config = Config::create_from_stream(
            profile("active_displays: []\nactive_views: []\n").as_bytes(),
        )
        .unwrap();
        assert_eq!(config.default_view("sRGB_1"), b"View_2");
        assert_eq!(config.num_views("sRGB_1"), 1);
        assert_eq!(config.view("sRGB_1", 0), b"View_2");
        assert_eq!(config.default_view("sRGB_2"), b"View_3");
        assert_eq!(config.num_views("sRGB_2"), 2);
        assert_eq!(config.view("sRGB_2", 0), b"View_3");
        assert_eq!(config.view("sRGB_2", 1), b"View_2");
        assert_eq!(config.default_view("sRGB_3"), b"View_3");
        assert_eq!(config.num_views("sRGB_3"), 1);
        assert_eq!(config.view("sRGB_3", 0), b"View_3");
    }

    // No value, and no value but a misleading space.
    for value in ["", " "] {
        env.set(&[("OCIO_ACTIVE_VIEWS", value)]);
        let config = Config::create_from_stream(
            profile("active_displays: []\nactive_views: []\n").as_bytes(),
        )
        .unwrap();
        assert_eq!(config.default_view("sRGB_1"), b"View_1");
        assert_eq!(config.num_views("sRGB_1"), 2);
        assert_eq!(config.view("sRGB_1", 0), b"View_1");
        assert_eq!(config.view("sRGB_1", 1), b"View_2");
        assert_eq!(config.default_view("sRGB_2"), b"View_2");
        assert_eq!(config.num_views("sRGB_2"), 2);
        assert_eq!(config.view("sRGB_2", 0), b"View_2");
        assert_eq!(config.view("sRGB_2", 1), b"View_3");
        assert_eq!(config.default_view("sRGB_3"), b"View_3");
        assert_eq!(config.num_views("sRGB_3"), 2);
        assert_eq!(config.view("sRGB_3", 0), b"View_3");
        assert_eq!(config.view("sRGB_3", 1), b"View_1");
    }
}

/// Port of `OCIO_ADD_TEST(Config, key_value_error)` @ v2.5.2.
#[test]
fn key_value_error() {
    let _env = EnvGuard::new();
    // Check the line number contained in the parser error messages.

    const SHORT_PROFILE: &str = "ocio_profile_version: 2\n\
strictparsing: false\n\
roles:\n  \
default: raw\n\
displays:\n  \
sRGB:\n  \
- !<View> {name: Raw, colorspace: raw}\n\
\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n    \
to_scene_reference: !<MatrixTransform> \n                      \
{\n                           \
matrix: [1, 0, 0, 0, 0, 1]\n                      \
}\n    \
allocation: uniform\n\
\n";

    check_throw_what(
        Config::create_from_stream(SHORT_PROFILE.as_bytes()),
        "Error: Loading the OCIO profile failed. At line 14, the value parsing of the key \
         'matrix' from 'MatrixTransform' failed: 'matrix' values must be 16 numbers. Found '6'.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, unknown_key_error)` @ v2.5.2.
#[test]
fn unknown_key_error() {
    let _env = EnvGuard::new();
    let oss = profile_v2_start() + "    dummyKey: dummyValue\n";

    let (config, log) = crate::test_env::capture_log(|| Config::create_from_stream(oss.as_bytes()));
    assert!(config.is_ok());
    let output = log.concat();
    assert!(output.starts_with(
        b"[OpenColorIO Warning]: At line 56, unknown key 'dummyKey' in 'ColorSpace'."
    ));
}

/// Port of `OCIO_ADD_TEST(Config, faulty_config_file)` @ v2.5.2.
#[test]
fn faulty_config_file() {
    let _env = EnvGuard::new();
    check_throw_what(
        Config::create_from_stream(b"/usr/tmp/not_existing.ocio"),
        "Error: Loading the OCIO profile failed.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, display_color_spaces_errors)` @ v2.5.2.
#[test]
fn display_color_spaces_errors() {
    let _env = EnvGuard::new();
    {
        const STR_DCS: &str = "\n\
display_colorspaces:\n  \
- !<ColorSpace>\n    \
name: dcs1\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n    \
from_scene_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}\n\
\n  \
- !<ColorSpace>\n    \
name: dcs2\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n    \
to_display_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1]}\n";
        let s = profile_v2_dcs_start() + STR_DCS + SIMPLE_PROFILE_CS_V2;

        check_throw_what(
            Config::create_from_stream(s.as_bytes()),
            "'from_scene_reference' cannot be used for a display color space",
        );
    }
    {
        const STR_DCS: &str = "\n\
display_colorspaces:\n  \
- !<ColorSpace>\n    \
name: dcs1\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n    \
from_display_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1], direction: inverse}\n\
\n  \
- !<ColorSpace>\n    \
name: dcs2\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n    \
to_scene_reference: !<ExponentTransform> {value: [2.4, 2.4, 2.4, 1]}\n";
        let s = profile_v2_dcs_start() + STR_DCS + SIMPLE_PROFILE_CS_V2;

        check_throw_what(
            Config::create_from_stream(s.as_bytes()),
            "'to_scene_reference' cannot be used for a display color space",
        );
    }
}

// The tests that serialize configs (WP 3.7).

/// `config.serialize()`, split by lines (upstream's `serialize(os)` and
/// `StringUtils::SplitByLines(os.str())`).
fn serialized_lines(config: &Config) -> Vec<String> {
    split_by_lines(&config.serialize().unwrap())
        .into_iter()
        .map(|l| String::from_utf8(l).unwrap())
        .collect()
}

/// Upstream's check of a serialized config against its expected text: the same number of
/// lines, and each line equal.
fn check_profile_lines(config: &Config, expected: &str) {
    let osvec = serialized_lines(config);
    let profile_outvec: Vec<String> = split_by_lines(expected.as_bytes())
        .into_iter()
        .map(|l| String::from_utf8(l).unwrap())
        .collect();
    assert_eq!(osvec.len(), profile_outvec.len());
    for (os, expected) in osvec.iter().zip(&profile_outvec) {
        assert_eq!(os, expected);
    }
}

/// Port of `OCIO_ADD_TEST(Config, serialize_group_transform)` @ v2.5.2.
#[test]
fn serialize_group_transform() {
    let _env = EnvGuard::new();
    // The unit test validates that a group transform is correctly serialized.

    let mut config = Config::new().unwrap();
    {
        let mut cs = ColorSpace::new();
        cs.set_name("testing");
        cs.set_family("test");
        let mut group_transform = GroupTransform::new();
        // Default and unknown interpolation are not saved.
        let transform1 = FileTransform::new();
        group_transform.append_transform(transform1.into());
        let mut transform2 = FileTransform::new();
        transform2.set_interpolation(Interpolation::Unknown);
        group_transform.append_transform(transform2.into());
        let mut transform3 = FileTransform::new();
        transform3.set_interpolation(Interpolation::Best);
        group_transform.append_transform(transform3.into());
        let mut transform4 = FileTransform::new();
        transform4.set_interpolation(Interpolation::Nearest);
        group_transform.append_transform(transform4.into());
        let mut transform5 = FileTransform::new();
        transform5.set_interpolation(Interpolation::Cubic);
        group_transform.append_transform(transform5.into());
        cs.set_transform(
            Some(&group_transform.into()),
            ColorSpaceDirection::FromReference,
        )
        .unwrap();
        config.add_color_space(&cs).unwrap();
        config.set_role(ROLE_DEFAULT, Some(cs.name())).unwrap();
        config
            .set_role(ROLE_COMPOSITING_LOG, Some(cs.name()))
            .unwrap();
    }
    {
        let mut cs = ColorSpace::new();
        cs.set_name("testing2");
        cs.set_family("test");
        let transform1 = ExponentTransform::new();
        let mut group_transform = GroupTransform::new();
        group_transform.append_transform(transform1.into());
        cs.set_transform(
            Some(&group_transform.into()),
            ColorSpaceDirection::ToReference,
        )
        .unwrap();
        config.add_color_space(&cs).unwrap();
        // Replace the role.
        config
            .set_role(ROLE_COMPOSITING_LOG, Some(cs.name()))
            .unwrap();
    }

    config.set_version(2, 2).unwrap();

    const PROFILE_OUT: &str = concat!(
        "ocio_profile_version: 2.2\n",
        "\n",
        "environment:\n",
        "  {}\n",
        "search_path: \"\"\n",
        "strictparsing: true\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "\n",
        "roles:\n",
        "  compositing_log: testing2\n",
        "  default: testing\n",
        "\n",
        "file_rules:\n",
        "  - !<Rule> {name: Default, colorspace: default}\n",
        "\n",
        "displays:\n",
        "  {}\n",
        "\n",
        "active_displays: []\n",
        "active_views: []\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: testing\n",
        "    family: test\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    allocation: uniform\n",
        "    from_scene_reference: !<GroupTransform>\n",
        "      children:\n",
        "        - !<FileTransform> {src: \"\"}\n",
        "        - !<FileTransform> {src: \"\", interpolation: unknown}\n",
        "        - !<FileTransform> {src: \"\", interpolation: best}\n",
        "        - !<FileTransform> {src: \"\", interpolation: nearest}\n",
        "        - !<FileTransform> {src: \"\", interpolation: cubic}\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: testing2\n",
        "    family: test\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    allocation: uniform\n",
        "    to_scene_reference: !<GroupTransform>\n",
        "      children:\n",
        "        - !<ExponentTransform> {value: 1}\n",
    );

    check_profile_lines(&config, PROFILE_OUT);
}

/// Port of `OCIO_ADD_TEST(Config, serialize_searchpath)` @ v2.5.2.
#[test]
fn serialize_searchpath() {
    let _env = EnvGuard::new();
    {
        let mut config = Config::new().unwrap();
        {
            let mut cs = ColorSpace::new();
            cs.set_name("default");
            cs.set_is_data(true);
            config.add_color_space(&cs).unwrap();
            config.set_version(2, 2).unwrap();
        }

        const PROFILE_OUT: &str = concat!(
            "ocio_profile_version: 2.2\n",
            "\n",
            "environment:\n",
            "  {}\n",
            "search_path: \"\"\n",
            "strictparsing: true\n",
            "luma: [0.2126, 0.7152, 0.0722]\n",
            "\n",
            "roles:\n",
            "  {}\n",
            "\n",
            "file_rules:\n",
            "  - !<Rule> {name: Default, colorspace: default}\n",
            "\n",
            "displays:\n",
            "  {}\n",
            "\n",
            "active_displays: []\n",
            "active_views: []\n",
            "\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: default\n",
            "    family: \"\"\n",
            "    equalitygroup: \"\"\n",
            "    bitdepth: unknown\n",
            "    isdata: true\n",
            "    allocation: uniform\n",
        );

        check_profile_lines(&config, PROFILE_OUT);
    }

    {
        let mut config = Config::new().unwrap();
        config
            .set_major_version(FIRST_SUPPORTED_MAJOR_VERSION)
            .unwrap();
        config.set_minor_version(0).unwrap();

        let search_path = "a:b:c";
        config.set_search_path(search_path);

        let osvec = serialized_lines(&config);

        // V1 saves search_path as a single string.
        assert_eq!(osvec[2], "search_path: a:b:c");

        // V2 saves search_path as separate strings.
        config.set_major_version(2).unwrap();
        let os = config.serialize().unwrap();
        let osvec = serialized_lines(&config);

        let expected2 = ["search_path:", "  - a", "  - b", "  - c"];
        assert_eq!(osvec[4], expected2[0]);
        assert_eq!(osvec[5], expected2[1]);
        assert_eq!(osvec[6], expected2[2]);
        assert_eq!(osvec[7], expected2[3]);

        let config_read = Config::create_from_stream(&os).unwrap();

        assert_eq!(config_read.num_search_paths(), 3);
        assert_eq!(config_read.search_path(), search_path.as_bytes());
        assert_eq!(config_read.search_path_with_index(0), b"a");
        assert_eq!(config_read.search_path_with_index(1), b"b");
        assert_eq!(config_read.search_path_with_index(2), b"c");

        config.clear_search_paths();
        let sp0 = "a path with a - in it/";
        let sp1 = "/absolute/linux/path";
        let sp2 = "C:\\absolute\\windows\\path";
        let sp3 = "!<path> using /yaml/symbols";
        config.add_search_path(sp0);
        config.add_search_path(sp1);
        config.add_search_path(sp2);
        config.add_search_path(sp3);
        let os = config.serialize().unwrap();

        let osvec = serialized_lines(&config);

        let expected3 = [
            "search_path:",
            "  - a path with a - in it/",
            "  - /absolute/linux/path",
            "  - C:\\absolute\\windows\\path",
            "  - \"!<path> using /yaml/symbols\"",
        ];
        assert_eq!(osvec[4], expected3[0]);
        assert_eq!(osvec[5], expected3[1]);
        assert_eq!(osvec[6], expected3[2]);
        assert_eq!(osvec[7], expected3[3]);
        assert_eq!(osvec[8], expected3[4]);

        let config_read = Config::create_from_stream(&os).unwrap();

        assert_eq!(config_read.num_search_paths(), 4);
        assert_eq!(config_read.search_path_with_index(0), sp0.as_bytes());
        assert_eq!(config_read.search_path_with_index(1), sp1.as_bytes());
        assert_eq!(config_read.search_path_with_index(2), sp2.as_bytes());
        assert_eq!(config_read.search_path_with_index(3), sp3.as_bytes());
    }
}

/// Port of `OCIO_ADD_TEST(Config, serialize_environment)` @ v2.5.2.
#[test]
fn serialize_environment() {
    let _env = EnvGuard::new();
    {
        let mut config = Config::new().unwrap();
        config.set_major_version(1).unwrap();
        config.set_minor_version(0).unwrap();

        let osvec = serialized_lines(&config);

        // A v1 config does not write the environment section if it's empty.
        assert_eq!(osvec[2], "search_path: \"\"");
    }
    {
        let mut config = Config::new().unwrap();
        config.set_major_version(2).unwrap();
        config.set_minor_version(0).unwrap();

        let osvec = serialized_lines(&config);

        // A v2 config does write the environment section, even if it's empty.
        assert_eq!(osvec[2], "environment:");
        assert_eq!(osvec[3], "  {}");
    }
    {
        let mut config = Config::new().unwrap();
        config.set_major_version(1).unwrap();
        config.set_minor_version(0).unwrap();

        config.add_environment_var("SHOT", Some(b"0001"));

        let osvec = serialized_lines(&config);

        // A v1 config does write the environment section if it's not empty.
        assert_eq!(osvec[2], "environment:");
        assert_eq!(osvec[3], "  SHOT: 0001");
    }
}

/// Port of `OCIO_ADD_TEST(Config, active_displayview_lists)` @ v2.5.2.
#[test]
fn active_displayview_lists() {
    let _env = EnvGuard::new();
    let mut config = (*Config::create_raw().unwrap()).clone();
    let display = |c: &Config, i: i32| c.active_display(i).unwrap().to_vec();
    let view = |c: &Config, i: i32| c.active_view(i).unwrap().to_vec();

    // Test add.
    assert_eq!(config.num_active_displays(), 0);
    assert_eq!(config.num_active_views(), 0);
    config.add_active_display("sRGB").unwrap();
    config.add_active_display("Display P3").unwrap();
    config.add_active_view("v1").unwrap();
    config.add_active_view("v2").unwrap();

    // Test getter.
    assert_eq!(config.num_active_displays(), 2);
    assert_eq!(display(&config, 0), b"sRGB");
    assert_eq!(display(&config, 1), b"Display P3");
    assert_eq!(config.num_active_views(), 2);
    assert_eq!(view(&config, 0), b"v1");
    assert_eq!(view(&config, 1), b"v2");

    // Trying to add one that is already present doesn't add one, but does not throw.
    config.add_active_display("sRGB").unwrap();
    config.add_active_view("v1").unwrap();
    assert_eq!(config.num_active_displays(), 2);
    assert_eq!(config.num_active_views(), 2);

    // Test commas may be used.
    config
        .set_active_displays("sRGB:01, \"Name, with comma\", \"Quoted name\"")
        .unwrap();
    assert_eq!(config.num_active_displays(), 3);
    assert_eq!(display(&config, 0), b"sRGB:01");
    assert_eq!(display(&config, 1), b"Name, with comma");
    config
        .set_active_views("v:01, \"View, with comma\", \"Quoted view\"")
        .unwrap();
    assert_eq!(config.num_active_views(), 3);
    assert_eq!(view(&config, 0), b"v:01");
    assert_eq!(view(&config, 1), b"View, with comma");

    // Test remove.
    config.remove_active_display("Name, with comma").unwrap();
    assert_eq!(config.num_active_displays(), 2);
    assert_eq!(display(&config, 1), b"Quoted name");
    config.remove_active_view("View, with comma").unwrap();
    assert_eq!(config.num_active_views(), 2);
    assert_eq!(view(&config, 1), b"Quoted view");

    // Test clear.
    config.clear_active_displays();
    assert_eq!(config.num_active_displays(), 0);
    config.clear_active_views();
    assert_eq!(config.num_active_views(), 0);

    // Trying to remove one that doesn't exist throws.
    check_throw_what(
        config.remove_active_display("not found"),
        "Active display could not be removed from config",
    );
    check_throw_what(
        config.remove_active_view("not found"),
        "Active view could not be removed from config",
    );

    // Test setting an empty string behaves as expected.
    config.set_active_displays("").unwrap();
    assert_eq!(config.num_active_displays(), 0);
    config.add_active_display("sRGB").unwrap();
    assert_eq!(config.num_active_displays(), 1);
    config.set_active_views("").unwrap();
    assert_eq!(config.num_active_views(), 0);
    config.add_active_view("v1").unwrap();
    assert_eq!(config.num_active_views(), 1);

    // Test commas may be serialized and restored.
    {
        config
            .set_active_displays("sRGB:01, \"Name, with comma\", \"Quoted name\"")
            .unwrap();
        config
            .set_active_views("v:01, \"View, with comma\", \"Quoted view\"")
            .unwrap();
        let oss = config.serialize().unwrap();
        let config2 = Config::create_from_stream(&oss).unwrap();
        assert_eq!(config2.num_active_displays(), 3);
        assert_eq!(display(&config2, 0), b"sRGB:01");
        assert_eq!(display(&config2, 1), b"Name, with comma");
        assert_eq!(display(&config2, 2), b"Quoted name");
        assert_eq!(config2.num_active_views(), 3);
        assert_eq!(view(&config2, 0), b"v:01");
        assert_eq!(view(&config2, 1), b"View, with comma");
        assert_eq!(view(&config2, 2), b"Quoted view");
    }

    // Check how an active list that uses colons as the separator is serialized.
    // Turns out these are serialized as commas, so this was never a viable method to
    // set an active list to handle use of commas in names. (It would be ok for use in
    // the env. var., but not in the config itself.)
    {
        config
            .set_active_displays("sRGB01 : Name : \"Quoted name\"")
            .unwrap();
        config
            .set_active_views("v01:View: \"Quoted view\"")
            .unwrap();
        let oss = config.serialize().unwrap();
        let config2 = Config::create_from_stream(&oss).unwrap();
        assert_eq!(config2.num_active_displays(), 3);
        assert_eq!(config2.active_displays(), b"sRGB01, Name, Quoted name");
        assert_eq!(config2.num_active_views(), 3);
        assert_eq!(config2.active_views(), b"v01, View, Quoted view");
    }
}

/// Port of `OCIO_ADD_TEST(Config, file_transform_serialization_v1)` @ v2.5.2.
#[test]
fn file_transform_serialization_v1() {
    let _env = EnvGuard::new();
    let mut cfg = Config::new().unwrap();
    cfg.set_major_version(1).unwrap();
    let mut ft = FileTransform::new();
    ft.set_src("file");
    let mut cs = ColorSpace::new();
    // Note that ft has no interpolation set.  In a v2 config, this is not a problem and is taken
    // to mean default interpolation.  However, in this case the config version is 1 and if the
    // config were read by a v1 library (rather than v2), this could cause a failure.  So the
    // interp is set to linear during serialization to avoid problems.
    cs.set_transform(Some(&ft.clone().into()), ColorSpaceDirection::ToReference)
        .unwrap();
    ft.set_src("other");
    ft.set_interpolation(Interpolation::Tetrahedral);
    cs.set_transform(Some(&ft.into()), ColorSpaceDirection::FromReference)
        .unwrap();
    cs.set_name("cs");
    cfg.add_color_space(&cs).unwrap();
    let os = cfg.serialize().unwrap();
    assert_eq!(
        String::from_utf8(os).unwrap(),
        r#"ocio_profile_version: 1

search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  {}

displays:
  {}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: cs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_reference: !<FileTransform> {src: file, interpolation: linear}
    from_reference: !<FileTransform> {src: other, interpolation: tetrahedral}
"#
    );
}

/// Port of `OCIO_ADD_TEST(Config, transform_versions)` @ v2.5.2.
#[test]
fn transform_versions() {
    let _env = EnvGuard::new();
    // Saving a v1 config containing v2 transforms must fail.

    let mut config = Config::new().unwrap();
    // OCIO_VERSION_MAJOR.
    assert_eq!(config.major_version(), LAST_SUPPORTED_MAJOR_VERSION);

    config
        .set_major_version(FIRST_SUPPORTED_MAJOR_VERSION)
        .unwrap();
    config.set_minor_version(0).unwrap();

    assert_eq!(config.major_version(), 1);

    let range = RangeTransform::new();

    let mut cs = ColorSpace::new();
    cs.set_name("range");
    cs.set_transform(Some(&range.into()), ColorSpaceDirection::ToReference)
        .unwrap();

    config.add_color_space(&cs).unwrap();

    check_throw_what(
        config.serialize(),
        "Error building YAML: Only config version 2 (or higher) can have RangeTransform.",
    );

    // Loading a v1 config containing v2 transforms must fail.

    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 1

roles:
  default: raw

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform
    from_reference: !<GroupTransform>
       children:
         - !<RangeTransform> {min_in_value: 0, min_out_value: 0}
"#;

    check_throw_what(
        Config::create_from_stream(OCIO_CONFIG.as_bytes()),
        "Only config version 2 (or higher) can have RangeTransform.",
    );

    // NOTE: For more tests of Config::Impl::checkVersionConsistency(ConstTransformRcPtr & transform)
    // for Builtin Transform styles, please see BuiltinTransformRegistry_tests.cpp.
}

/// Port of `OCIO_ADD_TEST(Config, description_and_name)` @ v2.5.2.
#[test]
fn description_and_name() {
    let _env = EnvGuard::new();
    let mut cfg = (*Config::create_raw().unwrap()).clone();
    const CONFIG_NO_DESC: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

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
    assert_eq!(
        String::from_utf8(cfg.serialize().unwrap()).unwrap(),
        CONFIG_NO_DESC
    );

    cfg.set_description("single line description");
    cfg.set_name("Test config name");

    // Verify name is copied.
    {
        let cfg2 = cfg.clone();
        assert_eq!(cfg2.name(), b"Test config name");
    }

    const CONFIG_DESC_SINGLELINE: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]
name: Test config name
description: single line description

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

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
    assert_eq!(
        String::from_utf8(cfg.serialize().unwrap()).unwrap(),
        CONFIG_DESC_SINGLELINE
    );

    cfg.set_description("multi line description\n\nother line");
    cfg.set_name("");

    const CONFIG_DESC_MULTILINES: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]
description: |
  multi line description

  other line

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

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
    assert_eq!(
        String::from_utf8(cfg.serialize().unwrap()).unwrap(),
        CONFIG_DESC_MULTILINES
    );
}

/// Port of `OCIO_ADD_TEST(Config, internal_raw_profile)` @ v2.5.2.
#[test]
fn internal_raw_profile() {
    let _env = EnvGuard::new();
    Config::create_from_stream(INTERNAL_RAW_PROFILE.as_bytes()).unwrap();
}

/// Port of `OCIO_ADD_TEST(Config, compare_displays)` @ v2.5.2 (Display_tests.cpp).
#[test]
fn compare_displays() {
    let _env = EnvGuard::new();
    const CONFIG1: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
    - !<Views> [sview1]

active_displays: [sRGB]
active_views: [view, sview1]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

    const CONFIG2: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<Views> [view, sview1]

active_displays: [Raw]
active_views: [Raw]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

    let config1 = Config::create_from_stream(CONFIG1.as_bytes()).unwrap();
    let config2 = Config::create_from_stream(CONFIG2.as_bytes()).unwrap();
    config1.validate().unwrap();
    config2.validate().unwrap();

    check_compare_displays((*config1).clone(), (*config2).clone());
}

/// Port of `OCIO_ADD_TEST(Config, compare_virtual_displays)` @ v2.5.2 (Display_tests.cpp).
#[test]
fn compare_virtual_displays() {
    let _env = EnvGuard::new();
    const CONFIG1: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

viewing_rules:
  - !<Rule> {name: Linear, colorspaces: default}

shared_views:
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>, looks: look1, rule: Linear, description: Test view}
  - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [Film, view]

looks:
  - !<Look>
    name: look1
    process_space: default

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

    const CONFIG2: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

viewing_rules:
  - !<Rule> {name: Linear, colorspaces: default}

shared_views:
  - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<Views> [view]

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>, looks: look1, rule: Linear, description: Test view}
  - !<Views> [view]

looks:
  - !<Look>
    name: look1
    process_space: default

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

    let config1 = Config::create_from_stream(CONFIG1.as_bytes()).unwrap();
    let config2 = Config::create_from_stream(CONFIG2.as_bytes()).unwrap();
    config1.validate().unwrap();
    config2.validate().unwrap();

    // The getters of a virtual view: view transform, color space, looks, rule, description.
    let props = |c: &Config, v: &[u8]| {
        [
            c.virtual_display_view_transform_name(v).to_vec(),
            c.virtual_display_view_color_space_name(v).to_vec(),
            c.virtual_display_view_looks(v).to_vec(),
            c.virtual_display_view_rule(v).to_vec(),
            c.virtual_display_view_description(v).to_vec(),
        ]
    };
    let strs = |a: [&str; 5]| a.map(|s| s.as_bytes().to_vec());

    {
        // Test that Config::AreVirtualViewsEqual works for a matching virtual view pair across
        // separate configs. Works regardless of if the virtual view is display-defined in one
        // config and shared in the other.

        // Virtual view is a reference to a shared view.
        assert_eq!(2, config1.virtual_display_num_views(ViewType::Shared));

        let view_name1 = config1.virtual_display_view(ViewType::Shared, 0).to_vec();

        assert_eq!(b"Film", view_name1.as_slice());
        assert_eq!(
            props(&config1, &view_name1),
            strs([
                "display_vt",
                "<USE_DISPLAY_NAME>",
                "look1",
                "Linear",
                "Test view"
            ])
        );

        // Virtual view is a reference to a display-defined view.
        assert_eq!(
            2,
            config2.virtual_display_num_views(ViewType::DisplayDefined)
        );

        let view_name2 = config2
            .virtual_display_view(ViewType::DisplayDefined, 1)
            .to_vec();

        assert_eq!(b"Film", view_name2.as_slice());
        assert_eq!(
            props(&config2, &view_name2),
            strs([
                "display_vt",
                "<USE_DISPLAY_NAME>",
                "look1",
                "Linear",
                "Test view"
            ])
        );

        assert_eq!(view_name1, view_name2);
        assert!(Config::are_virtual_views_equal(
            &config1,
            &config2,
            &view_name1
        ));
    }
    {
        // Virtual views are both display-defined.
        assert_eq!(
            1,
            config1.virtual_display_num_views(ViewType::DisplayDefined)
        );

        let view_name1 = config1
            .virtual_display_view(ViewType::DisplayDefined, 0)
            .to_vec();

        assert_eq!(b"Raw", view_name1.as_slice());
        assert_eq!(props(&config1, &view_name1), strs(["", "raw", "", "", ""]));

        let view_name2 = config2
            .virtual_display_view(ViewType::DisplayDefined, 0)
            .to_vec();

        assert_eq!(b"Raw", view_name2.as_slice());
        assert_eq!(props(&config2, &view_name2), strs(["", "raw", "", "", ""]));

        assert_eq!(view_name1, view_name2);
        assert!(Config::are_virtual_views_equal(
            &config1,
            &config2,
            &view_name1
        ));
    }
    {
        // Virtual views are both shared.
        let view_name1 = config1.virtual_display_view(ViewType::Shared, 1).to_vec();

        assert_eq!(b"view", view_name1.as_slice());
        assert_eq!(
            props(&config1, &view_name1),
            strs(["display_vt", "display_cs", "", "", ""])
        );

        assert_eq!(1, config2.virtual_display_num_views(ViewType::Shared));

        let view_name2 = config2.virtual_display_view(ViewType::Shared, 0).to_vec();

        assert_eq!(b"view", view_name2.as_slice());
        assert_eq!(
            props(&config2, &view_name2),
            strs(["display_vt", "display_cs", "", "", ""])
        );

        assert_eq!(view_name1, view_name2);
        assert!(Config::are_virtual_views_equal(
            &config1,
            &config2,
            &view_name1
        ));

        assert_eq!(view_name1, view_name2);
        assert!(Config::are_virtual_views_equal(
            &config1,
            &config2,
            &view_name1
        ));
    }
    {
        // Test when a shared virtual view exists in one config but not the other.
        let mut cfg = (*config1).clone();

        assert!(config1.has_virtual_view("Film"));
        assert!(config1.is_virtual_view_shared("Film"));

        assert_eq!(2, cfg.virtual_display_num_views(ViewType::Shared));
        assert!(cfg.has_virtual_view("Film"));
        assert!(cfg.is_virtual_view_shared("Film"));

        assert!(Config::are_virtual_views_equal(&config1, &cfg, "Film"));

        // Check against another config where the virtual view is display-defined.
        assert!(Config::are_virtual_views_equal(&config2, &cfg, "Film"));

        // Remove a shared view from the virtual display.
        cfg.remove_virtual_display_view("Film");

        assert_eq!(1, cfg.virtual_display_num_views(ViewType::Shared));
        assert!(!cfg.has_virtual_view("Film"));
        assert!(!cfg.is_virtual_view_shared("Film"));

        assert!(!Config::are_virtual_views_equal(&config1, &cfg, "Film"));
        assert!(!Config::are_virtual_views_equal(&config2, &cfg, "Film"));
    }
    {
        // Test when a display-defined virtual view exists in one config but not the other.
        let mut cfg = (*config2).clone();

        // Remove a display-defined view from the virtual display.
        assert!(config2.has_virtual_view("Film"));
        assert!(!config2.is_virtual_view_shared("Film")); // Confirm display-defined

        assert_eq!(2, cfg.virtual_display_num_views(ViewType::DisplayDefined));
        assert!(cfg.has_virtual_view("Film"));
        assert!(!cfg.is_virtual_view_shared("Film")); // Confirm display-defined

        assert!(Config::are_virtual_views_equal(&config2, &cfg, "Film"));

        // Check against another config where the virtual view is a reference to a shared view.
        assert!(Config::are_virtual_views_equal(&config1, &cfg, "Film"));

        // Remove a display-defined view from the virtual display.
        cfg.remove_virtual_display_view("Film");

        assert_eq!(1, cfg.virtual_display_num_views(ViewType::DisplayDefined));
        assert!(!cfg.has_virtual_view("Film"));

        assert!(!Config::are_virtual_views_equal(&config2, &cfg, "Film"));
        assert!(!Config::are_virtual_views_equal(&config1, &cfg, "Film"));
    }
}

// Group B: the tests that validate configs (WP 3.8).

/// Port of `OCIO_ADD_TEST(Config, simple_config)` @ v2.5.2.
#[test]
fn simple_config() {
    let _env = EnvGuard::new();
    const SIMPLE_PROFILE: &str = concat!(
        "ocio_profile_version: 1\n",
        "resource_path: luts\n",
        "strictparsing: false\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "roles:\n",
        "  default: raw\n",
        "  scene_linear: lnh\n",
        "displays:\n",
        "  sRGB:\n",
        "  - !<View> {name: Film1D, colorspace: loads_of_transforms}\n",
        "  - !<View> {name: Ln, colorspace: lnh}\n",
        "  - !<View> {name: Raw, colorspace: raw}\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "      name: raw\n",
        "      family: raw\n",
        "      equalitygroup: \n",
        "      bitdepth: 32f\n",
        "      description: |\n",
        "        A raw color space. Conversions to and from this space are no-ops.\n",
        "      isdata: true\n",
        "      allocation: uniform\n",
        "  - !<ColorSpace>\n",
        "      name: lnh\n",
        "      family: ln\n",
        "      equalitygroup: \n",
        "      bitdepth: 16f\n",
        "      description: |\n",
        "        The show reference space. This is a sensor referred linear\n",
        "        representation of the scene with primaries that correspond to\n",
        "        scanned film. 0.18 in this space corresponds to a properly\n",
        "        exposed 18% grey card.\n",
        "      isdata: false\n",
        "      allocation: lg2\n",
        "  - !<ColorSpace>\n",
        "      name: loads_of_transforms\n",
        "      family: vd8\n",
        "      equalitygroup: \n",
        "      bitdepth: 8ui\n",
        "      description: 'how many transforms can we use?'\n",
        "      isdata: false\n",
        "      allocation: uniform\n",
        "      to_reference: !<GroupTransform>\n",
        "        direction: forward\n",
        "        children:\n",
        "          - !<FileTransform>\n",
        "            src: diffusemult.spimtx\n",
        "            interpolation: unknown\n",
        "          - !<ColorSpaceTransform>\n",
        "            src: raw\n",
        "            dst: lnh\n",
        "          - !<ExponentTransform>\n",
        "            value: [2.2, 2.2, 2.2, 1]\n",
        "          - !<MatrixTransform>\n",
        "            matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]\n",
        "            offset: [0, 0, 0, 0]\n",
        "          - !<CDLTransform>\n",
        "            slope: [1, 1, 1]\n",
        "            offset: [0, 0, 0]\n",
        "            power: [1, 1, 1]\n",
        "            saturation: 1\n",
        "\n",
    );

    let config = Config::create_from_stream(SIMPLE_PROFILE.as_bytes()).unwrap();
    config.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(Config, validation)` @ v2.5.2.
#[test]
fn validation() {
    let _env = EnvGuard::new();
    {
        let simple_profile = concat!(
            "ocio_profile_version: 1\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "      name: raw\n",
            "  - !<ColorSpace>\n",
            "      name: raw\n",
            "strictparsing: false\n",
            "roles:\n",
            "  default: raw\n",
            "displays:\n",
            "  sRGB:\n",
            "  - !<View> {name: Raw, colorspace: raw}\n",
            "\n",
        );

        check_throw_what(
            Config::create_from_stream(simple_profile.as_bytes()),
            "Colorspace with name 'raw' already defined",
        );
    }

    {
        let simple_profile = concat!(
            "ocio_profile_version: 1\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "      name: raw\n",
            "strictparsing: false\n",
            "roles:\n",
            "  default: raw\n",
            "displays:\n",
            "  sRGB:\n",
            "  - !<View> {name: Raw, colorspace: raw}\n",
            "\n",
        );

        let config = Config::create_from_stream(simple_profile.as_bytes()).unwrap();

        config.validate().unwrap();
    }
}

/// Port of `OCIO_ADD_TEST(Config, version)` @ v2.5.2.
#[test]
fn version() {
    let _env = EnvGuard::new();
    let simple_profile = concat!(
        "ocio_profile_version: 2\n",
        "environment:\n",
        "  {}\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "      name: raw\n",
        "strictparsing: false\n",
        "roles:\n",
        "  default: raw\n",
        "displays:\n",
        "  sRGB:\n",
        "  - !<View> {name: Raw, colorspace: raw}\n",
        "\n",
    );

    let mut config = (*Config::create_from_stream(simple_profile.as_bytes()).unwrap()).clone();

    config.validate().unwrap();

    config.set_major_version(1).unwrap();
    check_throw_what(
        config.set_major_version(20000),
        "version is 20000 where supported versions start at 1 and end at 2",
    );

    {
        check_throw_what(
            config.set_minor_version(1),
            "The minor version 1 is not supported for major version 1. Maximum minor version is 0",
        );
    }

    let starts_with_lowered = |config: &Config, prefix: &str| {
        lower(&config.serialize().unwrap()).starts_with(prefix.as_bytes())
    };

    {
        config.set_minor_version(0).unwrap();

        assert!(starts_with_lowered(&config, "ocio_profile_version: 1"));
    }

    {
        config.set_major_version(2).unwrap();

        assert!(starts_with_lowered(&config, "ocio_profile_version: 2"));
    }

    {
        check_throw_what(
            config.set_version(2, 9),
            "The minor version 9 is not supported for major version 2. Maximum minor version is 5",
        );

        config.set_major_version(2).unwrap();
        check_throw_what(
            config.set_minor_version(9),
            "The minor version 9 is not supported for major version 2. Maximum minor version is 5",
        );
    }

    {
        check_throw_what(
            config.set_version(3, 4),
            "version is 3 where supported versions start at 1 and end at 2",
        );
    }
}

/// Port of `OCIO_ADD_TEST(Config, version_validation)` @ v2.5.2.
#[test]
fn version_validation() {
    let _env = EnvGuard::new();
    const SIMPLE_PROFILE_END: &str = concat!(
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "      name: raw\n",
        "strictparsing: false\n",
        "roles:\n",
        "  default: raw\n",
        "displays:\n",
        "  sRGB:\n",
        "  - !<View> {name: Raw, colorspace: raw}\n",
        "\n",
    );
    let read = |version: &str| {
        Config::create_from_stream(
            format!("ocio_profile_version: {version}\n{SIMPLE_PROFILE_END}").as_bytes(),
        )
    };

    check_throw_what(
        read("2.0.1"),
        "does not appear to have a valid version 2.0.1",
    );

    check_throw_what(
        read("2.9"),
        "The minor version 9 is not supported for major version 2",
    );

    check_throw_what(
        read("3"),
        "The version is 3 where supported versions start at 1 and end at 2",
    );

    check_throw_what(
        read("3.0"),
        "The version is 3 where supported versions start at 1 and end at 2",
    );

    {
        let config = read("1.0").unwrap();
        assert_eq!(config.major_version(), 1);
        assert_eq!(config.minor_version(), 0);
    }

    {
        let config = read("2.0").unwrap();
        assert_eq!(config.major_version(), 2);
        assert_eq!(config.minor_version(), 0);
    }
}

/// Port of `OCIO_ADD_TEST(Config, config_v1)` @ v2.5.2.
#[test]
fn config_v1() {
    let _env = EnvGuard::new();
    const CONFIG: &str = concat!(
        "ocio_profile_version: 1\n",
        "strictparsing: false\n",
        "roles:\n",
        "  default: raw\n",
        "displays:\n",
        "  sRGB:\n",
        "  - !<View> {name: Raw, colorspace: raw}\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "      name: raw\n",
    );

    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    assert_eq!(config.num_view_transforms(), 0);
    assert_eq!(
        config.num_color_spaces_with(SearchReferenceSpaceType::Display, ColorSpaceVisibility::All),
        0
    );
}

/// Port of `OCIO_ADD_TEST(Config, not_case_sensitive)` @ v2.5.2.
#[test]
fn not_case_sensitive() {
    let _env = EnvGuard::new();
    // Validate that the color spaces and roles are case insensitive.

    let config = Config::create_from_stream(profile_v2_start().as_bytes()).unwrap();
    config.validate().unwrap();

    assert!(config.color_space("lnh").is_some());

    assert!(config.color_space("LNH").is_some());

    assert!(config.color_space("RaW").is_some());

    assert!(config.has_role("default"));
    assert!(config.has_role("Default"));
    assert!(config.has_role("DEFAULT"));

    assert!(config.has_role("scene_linear"));
    assert!(config.has_role("Scene_Linear"));

    assert!(!config.has_role("reference"));
    assert!(!config.has_role("REFERENCE"));
}

/// Port of `OCIO_ADD_TEST(Config, look_transform)` @ v2.5.2.
#[test]
fn look_transform() {
    let _env = EnvGuard::new();
    // Validate Config::validate() on config file containing look transforms.

    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 2

environment:
  {}

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
  - !<View> {name: View1, colorspace: raw, looks: look1}

looks:
  - !<Look>
    name: look1
    process_space: default
    transform: !<ColorSpaceTransform> {src: default, dst: raw}
  - !<Look>
    name: look2
    process_space: default
    transform: !<LookTransform> {src: default, dst: raw, looks:+look1}

colorspaces:
  - !<ColorSpace>
    name: raw
    allocation: uniform
"#;

    let config = Config::create_from_stream(OCIO_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(Config, add_remove_display)` @ v2.5.2.
#[test]
fn add_remove_display() {
    let _env = EnvGuard::new();
    let mut config = (*Config::create_raw().unwrap()).clone();
    config.validate().unwrap();

    assert_eq!(config.num_displays(), 1);
    assert_eq!(config.display(0), b"sRGB");
    assert_eq!(config.num_views("sRGB"), 1);
    assert_eq!(config.view("sRGB", 0), b"Raw");

    // Add a (display, view) pair.

    config
        .add_display_view("disp1", "view1", "raw", "")
        .unwrap();
    assert!(config.has_view("disp1", "view1"));
    assert_eq!(config.num_displays(), 2);
    assert_eq!(config.display(0), b"sRGB");
    assert_eq!(config.display(1), b"disp1");
    assert_eq!(config.num_views("disp1"), 1);

    // Remove a (display, view) pair.

    config.remove_display_view("disp1", "view1").unwrap();
    assert!(!config.has_view("disp1", "view1"));
    assert_eq!(config.num_displays(), 1);
    assert_eq!(config.display(0), b"sRGB");
}

/// `PROFILE_V1` (Config_tests.cpp:2192-2194 @ v2.5.2).
const PROFILE_V1: &str = "ocio_profile_version: 1\n\
\n";

/// `SIMPLE_PROFILE_CS_V1` (Config_tests.cpp:2245-2271 @ v2.5.2).
const SIMPLE_PROFILE_CS_V1: &str = "\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n\
\n  \
- !<ColorSpace>\n    \
name: log\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n    \
from_reference: !<LogTransform> {base: 10}\n\
\n  \
- !<ColorSpace>\n    \
name: lnh\n    \
family: \"\"\n    \
equalitygroup: \"\"\n    \
bitdepth: unknown\n    \
isdata: false\n    \
allocation: uniform\n";

/// `PROFILE_V1 + SIMPLE_PROFILE_A + SIMPLE_PROFILE_B_V1` (Config_tests.cpp:2303 @ v2.5.2).
fn profile_v1_start() -> String {
    [
        PROFILE_V1,
        SIMPLE_PROFILE_A,
        SIMPLE_PROFILE_DISPLAYS_LOOKS,
        SIMPLE_PROFILE_CS_V1,
    ]
    .concat()
}

/// Compares serialized bytes with upstream's expected text.
#[track_caller]
fn check_serialized(actual: &[u8], expected: &str) {
    ocio_testkit::compare::assert_text_eq(
        "serialize",
        expected,
        std::str::from_utf8(actual).unwrap(),
    );
}

/// Reads `text`, validates it, and checks that the config serializes back to `text`, as
/// upstream's serialization tests do.
fn check_round_trip(text: &str) {
    let config = Config::create_from_stream(text.as_bytes()).unwrap();
    config.validate().unwrap();
    check_serialized(&config.serialize().unwrap(), text);
}

/// Port of `OCIO_ADD_TEST(Config, colorspacename_with_reserved_token)` @ v2.5.2.
#[test]
fn colorspacename_with_reserved_token() {
    let _env = EnvGuard::new();
    // Using context variable tokens (i.e. $ and %) in color space names is forbidden.

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::new();
    cs.set_name("cs1$VAR");
    check_throw_what(
        cfg.add_color_space(&cs),
        "A color space name 'cs1$VAR' cannot contain a context variable reserved token i.e. % \
         or $.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, serialize_colorspace_displayview_transforms)` @ v2.5.2.
#[test]
fn serialize_colorspace_displayview_transforms() {
    let _env = EnvGuard::new();
    // Validate that a ColorSpaceTransform and DisplayViewTransform are correctly serialized.
    let str_end = concat!(
        "    from_scene_reference: !<GroupTransform>\n",
        "      children:\n",
        "        - !<ColorSpaceTransform> {src: raw, dst: log}\n",
        "        - !<ColorSpaceTransform> {src: raw, dst: log, direction: inverse}\n",
        "        - !<ColorSpaceTransform> {src: default, dst: log, data_bypass: false}\n",
        "        - !<DisplayViewTransform> {src: raw, display: sRGB, view: RawView}\n",
        "        - !<DisplayViewTransform> {src: default, display: sRGB, view: RawView, direction: inverse}\n",
        "        - !<DisplayViewTransform> {src: log, display: sRGB, view: RawView, looks_bypass: true, data_bypass: false}\n",
    );

    check_round_trip(&(profile_v2_start() + str_end));
}

/// Port of `OCIO_ADD_TEST(Config, matrix_serialization)` @ v2.5.2.
#[test]
fn matrix_serialization() {
    let _env = EnvGuard::new();
    let str_end = concat!(
        "    from_reference: !<GroupTransform>\n",
        "      children:\n",
        // Check the value serialization.
        "        - !<MatrixTransform> {matrix: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],",
        " offset: [-1, -2, -3, -4]}\n",
        // Check the value precision.
        "        - !<MatrixTransform> {offset: [0.123456789876, 1.23456789876, 12.3456789876, 123.456789876]}\n",
        "        - !<MatrixTransform> {matrix: [0.123456789876, 1.23456789876, 12.3456789876, 123.456789876, ",
        "1234.56789876, 12345.6789876, 123456.789876, 1234567.89876, ",
        "0, 0, 1, 0, 0, 0, 0, 1]}\n",
    );

    check_round_trip(&(profile_v1_start() + str_end));
}

/// Port of `OCIO_ADD_TEST(Config, cdl_serialization)` @ v2.5.2.
#[test]
fn cdl_serialization() {
    let _env = EnvGuard::new();
    // Config v2.
    {
        let str_end = concat!(
            "    from_scene_reference: !<GroupTransform>\n",
            "      children:\n",
            "        - !<CDLTransform> {slope: [1, 2, 1]}\n",
            "        - !<CDLTransform> {offset: [0.1, 0.2, 0.1]}\n",
            "        - !<CDLTransform> {power: [1.1, 1.2, 1.1]}\n",
            "        - !<CDLTransform> {sat: 0.1, direction: inverse}\n",
            "        - !<CDLTransform> {slope: [2, 2, 3], offset: [0.2, 0.3, 0.1], power: [1.2, 1.1, 1], sat: 0.2, style: asc}\n",
        );

        check_round_trip(&(profile_v2_start() + str_end));
    }

    // Config v1.
    {
        let str_end = concat!(
            "    from_reference: !<GroupTransform>\n",
            "      children:\n",
            "        - !<CDLTransform> {slope: [1, 2, 1]}\n",
            "        - !<CDLTransform> {offset: [0.1, 0.2, 0.1]}\n",
            "        - !<CDLTransform> {power: [1.1, 1.2, 1.1]}\n",
            "        - !<CDLTransform> {sat: 0.1}\n",
        );

        check_round_trip(&(profile_v1_start() + str_end));
    }
}

/// Port of `OCIO_ADD_TEST(Config, file_transform_serialization)` @ v2.5.2.
#[test]
fn file_transform_serialization() {
    let _env = EnvGuard::new();
    // Config v2.
    let str_end = concat!(
        "    from_scene_reference: !<GroupTransform>\n",
        "      children:\n",
        "        - !<FileTransform> {src: a.clf}\n",
        "        - !<FileTransform> {src: b.ccc, cccid: cdl1, interpolation: best}\n",
        "        - !<FileTransform> {src: b.ccc, cccid: cdl2, cdl_style: asc, interpolation: linear}\n",
        "        - !<FileTransform> {src: a.clf, direction: inverse}\n",
    );

    check_round_trip(&(profile_v2_start() + str_end));
}

/// Port of `OCIO_ADD_TEST(Config, add_color_space)` @ v2.5.2.
#[test]
fn add_color_space() {
    use crate::transforms::fixed_function_transform::FixedFunctionTransform;
    use ocio_ops::open_color_types::FixedFunctionStyle;

    let _env = EnvGuard::new();
    // The unit test validates that the color space is correctly added to the configuration.

    // Note that the new C++11 u8 notation for UTF-8 string literals is used
    // to partially validate non-english language support.

    let str = profile_v2_start()
        + "    from_scene_reference: !<MatrixTransform> {offset: [-1, -2, -3, -4]}\n";

    let mut config = (*Config::create_from_stream(str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 3);

    let mut cs = ColorSpace::new();
    cs.set_name("ast\u{e9}ro\u{ef}de"); // Color space name with accents.
    // Some accents and some money symbols.
    cs.set_description(
        "\u{e9} \u{c0} \u{c2} \u{c7} \u{c9} \u{c8} \u{e7} -- $ \u{20ac} \u{5186} \u{a3} \u{5143}",
    );

    let tr = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]).unwrap();

    cs.set_transform(Some(&Transform::from(tr)), ColorSpaceDirection::ToReference)
        .unwrap();

    let cs_name = "ast\u{e9}ro\u{ef}de";

    assert_eq!(config.index_for_color_space(cs_name), -1);
    config.add_color_space(&cs).unwrap();
    assert_eq!(config.index_for_color_space(cs_name), 3);

    let res = str
        + "\n"
        + "  - !<ColorSpace>\n"
        + "    name: "
        + cs_name
        + "\n"
        + "    family: \"\"\n"
        + "    equalitygroup: \"\"\n"
        + "    bitdepth: unknown\n"
        + "    description: \u{e9} \u{c0} \u{c2} \u{c7} \u{c9} \u{c8} \u{e7} -- $ \u{20ac} \u{5186} \u{a3} \u{5143}\n"
        + "    isdata: false\n"
        + "    allocation: uniform\n"
        + "    to_scene_reference: !<FixedFunctionTransform> {style: ACES_RedMod03}\n";

    check_serialized(&config.serialize().unwrap(), &res);

    config.remove_color_space(cs_name);
    assert_eq!(config.num_color_spaces(), 3);
    assert_eq!(config.index_for_color_space(cs_name), -1);

    config.clear_color_spaces();
    assert_eq!(config.num_color_spaces(), 0);
}

/// Port of `OCIO_ADD_TEST(Config, display_color_spaces_serialization)` @ v2.5.2.
#[test]
fn display_color_spaces_serialization() {
    let _env = EnvGuard::new();
    {
        let str_dcs = concat!(
            "\n",
            "view_transforms:\n",
            "  - !<ViewTransform>\n",
            "    name: display\n",
            "    from_display_reference: !<MatrixTransform> {}\n",
            "\n",
            "  - !<ViewTransform>\n",
            "    name: scene\n",
            "    from_scene_reference: !<MatrixTransform> {}\n",
            "\n",
            "display_colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: dcs1\n",
            "    family: \"\"\n",
            "    equalitygroup: \"\"\n",
            "    bitdepth: unknown\n",
            "    isdata: false\n",
            "    allocation: uniform\n",
            "    from_display_reference: !<ExponentTransform> {value: 2.4, direction: inverse}\n",
            "\n",
            "  - !<ColorSpace>\n",
            "    name: dcs2\n",
            "    family: \"\"\n",
            "    equalitygroup: \"\"\n",
            "    bitdepth: unknown\n",
            "    isdata: false\n",
            "    allocation: uniform\n",
            "    to_display_reference: !<ExponentTransform> {value: 2.4}\n",
        );

        let str = profile_v2_dcs_start() + str_dcs + SIMPLE_PROFILE_CS_V2;

        let config = Config::create_from_stream(str.as_bytes()).unwrap();
        config.validate().unwrap();

        let ss = config.serialize().unwrap();
        assert_eq!(ss.len(), str.len());
        check_serialized(&ss, &str);
    }
}

/// Port of `OCIO_ADD_TEST(Config, categories)` @ v2.5.2.
#[test]
fn categories() {
    let _env = EnvGuard::new();
    const MY_OCIO_CONFIG: &str = concat!(
        "ocio_profile_version: 2\n",
        "\n",
        "environment:\n",
        "  {}\n",
        "search_path: luts\n",
        "strictparsing: true\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "\n",
        "roles:\n",
        "  default: raw1\n",
        "  scene_linear: raw1\n",
        "\n",
        "file_rules:\n",
        "  - !<Rule> {name: Default, colorspace: default}\n",
        "\n",
        "displays:\n",
        "  sRGB:\n",
        "    - !<View> {name: Raw, colorspace: raw1}\n",
        "\n",
        "active_displays: []\n",
        "active_views: []\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: raw1\n",
        "    family: \"\"\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    categories: [rendering, linear]\n",
        "    encoding: scene-linear\n",
        "    allocation: uniform\n",
        "    allocationvars: [-0.125, 1.125]\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: raw2\n",
        "    family: \"\"\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    categories: [rendering]\n",
        "    encoding: data\n",
        "    allocation: uniform\n",
        "    allocationvars: [-0.125, 1.125]\n",
    );

    let config = Config::create_from_stream(MY_OCIO_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    // Test the serialization & deserialization.

    check_serialized(&config.serialize().unwrap(), MY_OCIO_CONFIG);

    // Test the config content.

    // Upstream's null category is the empty one.
    let css = config.color_spaces("");
    assert_eq!(css.num_color_spaces(), 2);
    let cs = css.color_space_by_index(0).unwrap();
    assert_eq!(cs.num_categories(), 2);
    assert_eq!(cs.category(0).unwrap(), b"rendering");
    assert_eq!(cs.category(1).unwrap(), b"linear");

    let css = config.color_spaces("linear");
    assert_eq!(css.num_color_spaces(), 1);
    let cs = css.color_space_by_index(0).unwrap();
    assert_eq!(cs.num_categories(), 2);
    assert_eq!(cs.category(0).unwrap(), b"rendering");
    assert_eq!(cs.category(1).unwrap(), b"linear");

    let css = config.color_spaces("rendering");
    assert_eq!(css.num_color_spaces(), 2);

    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(config.color_space_name_by_index(0), b"raw1");
    assert_eq!(config.color_space_name_by_index(1), b"raw2");
    assert_eq!(config.index_for_color_space("raw1"), 0);
    assert_eq!(config.index_for_color_space("raw2"), 1);
    let cs = config.color_space("raw1").unwrap();
    assert_eq!(cs.name(), b"raw1");
    assert_eq!(cs.encoding(), b"scene-linear");
    let cs = config.color_space("raw2").unwrap();
    assert_eq!(cs.name(), b"raw2");
    assert_eq!(cs.encoding(), b"data");
}

/// Port of `OCIO_ADD_TEST(Config, display_view_order)` @ v2.5.2.
#[test]
fn display_view_order() {
    let _env = EnvGuard::new();
    const SIMPLE_CONFIG: &str = r#"
        ocio_profile_version: 2

        environment:
          {}

        displays:
          sRGB_B:
            - !<View> {name: View_2, colorspace: raw}
            - !<View> {name: View_1, colorspace: raw}
          sRGB_D:
            - !<View> {name: View_2, colorspace: raw}
            - !<View> {name: View_3, colorspace: raw}
          sRGB_A:
            - !<View> {name: View_3, colorspace: raw}
            - !<View> {name: View_1, colorspace: raw}
          sRGB_C:
            - !<View> {name: View_4, colorspace: raw}
            - !<View> {name: View_1, colorspace: raw}

        colorspaces:
          - !<ColorSpace>
            name: raw
            allocation: uniform

          - !<ColorSpace>
            name: lnh
            allocation: uniform

        file_rules:
          - !<Rule> {name: Default, colorspace: raw}
        "#;

    let config = Config::create_from_stream(SIMPLE_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    assert_eq!(config.num_displays(), 4);

    // When active_displays is not defined, the displays are returned in config order.

    assert_eq!(config.default_display(), b"sRGB_B");

    assert_eq!(config.display(0), b"sRGB_B");
    assert_eq!(config.display(1), b"sRGB_D");
    assert_eq!(config.display(2), b"sRGB_A");
    assert_eq!(config.display(3), b"sRGB_C");

    // When active_views is not defined, the views are returned in config order.

    assert_eq!(config.default_view("sRGB_B"), b"View_2");

    assert_eq!(config.num_views("sRGB_B"), 2);
    assert_eq!(config.view("sRGB_B", 0), b"View_2");
    assert_eq!(config.view("sRGB_B", 1), b"View_1");
}

/// Upstream's `InactiveCSConfigStart` (Config_tests.cpp:5905-5927 @ v2.5.2).
const INACTIVE_CS_CONFIG_START: &str = concat!(
    "ocio_profile_version: 2\n",
    "\n",
    "environment:\n",
    "  {}\n",
    "search_path: luts\n",
    "strictparsing: true\n",
    "luma: [0.2126, 0.7152, 0.0722]\n",
    "\n",
    "roles:\n",
    "  default: raw\n",
    "  scene_linear: lnh\n",
    "\n",
    "file_rules:\n",
    "  - !<Rule> {name: Default, colorspace: default}\n",
    "\n",
    "displays:\n",
    "  sRGB:\n",
    "    - !<View> {name: Raw, colorspace: raw}\n",
    "    - !<View> {name: Lnh, colorspace: lnh, looks: beauty}\n",
    "\n",
    "active_displays: []\n",
    "active_views: []\n",
);

/// Upstream's `InactiveCSConfigEnd` (Config_tests.cpp:5929-5986 @ v2.5.2).
const INACTIVE_CS_CONFIG_END: &str = concat!(
    "\n",
    "looks:\n",
    "  - !<Look>\n",
    "    name: beauty\n",
    "    process_space: lnh\n",
    "    transform: !<CDLTransform> {slope: [1, 2, 1]}\n",
    "\n",
    "\n",
    "colorspaces:\n",
    "  - !<ColorSpace>\n",
    "    name: raw\n",
    "    family: \"\"\n",
    "    equalitygroup: \"\"\n",
    "    bitdepth: unknown\n",
    "    isdata: false\n",
    "    allocation: uniform\n",
    "\n",
    "  - !<ColorSpace>\n",
    "    name: lnh\n",
    "    family: \"\"\n",
    "    equalitygroup: \"\"\n",
    "    bitdepth: unknown\n",
    "    isdata: false\n",
    "    allocation: uniform\n",
    "\n",
    "  - !<ColorSpace>\n",
    "    name: cs1\n",
    "    aliases: [alias1]\n",
    "    family: \"\"\n",
    "    equalitygroup: \"\"\n",
    "    bitdepth: unknown\n",
    "    isdata: false\n",
    "    categories: [file-io]\n",
    "    allocation: uniform\n",
    "    from_scene_reference: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}\n",
    "\n",
    "  - !<ColorSpace>\n",
    "    name: cs2\n",
    "    family: \"\"\n",
    "    equalitygroup: \"\"\n",
    "    bitdepth: unknown\n",
    "    isdata: false\n",
    "    categories: [working-space]\n",
    "    allocation: uniform\n",
    "    from_scene_reference: !<CDLTransform> {offset: [0.2, 0.2, 0.2]}\n",
    "\n",
    "  - !<ColorSpace>\n",
    "    name: cs3\n",
    "    family: \"\"\n",
    "    equalitygroup: \"\"\n",
    "    bitdepth: unknown\n",
    "    isdata: false\n",
    "    categories: [cat3]\n",
    "    allocation: uniform\n",
    "    from_scene_reference: !<CDLTransform> {offset: [0.3, 0.3, 0.3]}\n",
);

/// Port of `OCIO_ADD_TEST(Config, inactive_color_space_precedence)` @ v2.5.2.
#[test]
fn inactive_color_space_precedence() {
    // EnvGuard::new() unsets OCIO_INACTIVE_COLORSPACES, as upstream's Platform::Unsetenv.
    let env = EnvGuard::new();
    // The test demonstrates that an API request supersedes the env. variable and the
    // config file contents.

    let config_str = [
        INACTIVE_CS_CONFIG_START,
        "inactive_colorspaces: [cs2]\n",
        INACTIVE_CS_CONFIG_END,
    ]
    .concat();

    let num = |c: &Config, v| c.num_color_spaces_with(SearchReferenceSpaceType::All, v);

    let config = (*Config::create_from_stream(config_str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();

    assert_eq!(num(&config, ColorSpaceVisibility::Inactive), 1);
    assert_eq!(num(&config, ColorSpaceVisibility::Active), 4);
    assert_eq!(num(&config, ColorSpaceVisibility::All), 5);

    assert_eq!(config.color_space_name_by_index(0), b"raw");
    assert_eq!(config.color_space_name_by_index(1), b"lnh");
    assert_eq!(config.color_space_name_by_index(2), b"cs1");
    assert_eq!(config.color_space_name_by_index(3), b"cs3");

    // Env. variable supersedes the config content.

    env.set(&[("OCIO_INACTIVE_COLORSPACES", "cs3, cs1, lnh")]);

    let mut config = (*Config::create_from_stream(config_str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();

    assert_eq!(num(&config, ColorSpaceVisibility::Inactive), 3);
    assert_eq!(num(&config, ColorSpaceVisibility::Active), 2);
    assert_eq!(num(&config, ColorSpaceVisibility::All), 5);

    assert_eq!(config.color_space_name_by_index(0), b"raw");
    assert_eq!(config.color_space_name_by_index(1), b"cs2");

    // An API request supersedes the lists from the env. variable and the config file.

    config.set_inactive_color_spaces("cs1, lnh");

    assert_eq!(num(&config, ColorSpaceVisibility::Inactive), 2);
    assert_eq!(num(&config, ColorSpaceVisibility::Active), 3);
    assert_eq!(num(&config, ColorSpaceVisibility::All), 5);

    assert_eq!(config.color_space_name_by_index(0), b"raw");
    assert_eq!(config.color_space_name_by_index(1), b"cs2");
    assert_eq!(config.color_space_name_by_index(2), b"cs3");
}

/// Port of `OCIO_ADD_TEST(Config, is_colorspace_used)` @ v2.5.2.
#[test]
fn is_colorspace_used() {
    let _env = EnvGuard::new();
    // Test Config::isColorSpaceUsed() i.e. a color space could be defined but not used.

    const CONFIG: &str = concat!(
        "ocio_profile_version: 2\n",
        "\n",
        "environment:\n",
        "  {}\n",
        "\n",
        "search_path: luts\n",
        "strictparsing: true\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "\n",
        "roles:\n",
        "  default: cs1\n",
        "\n",
        "view_transforms:\n",
        "  - !<ViewTransform>\n",
        "    name: vt1\n",
        "    from_scene_reference: !<ColorSpaceTransform> {src: cs11, dst: cs11}\n",
        "\n",
        "displays:\n",
        "  disp1:\n",
        "    - !<View> {name: view1, colorspace: cs2}\n",
        "    - !<View> {name: view2, colorspace: cs9}\n",
        "\n",
        "active_displays: [disp1]\n",
        "active_views: [view1]\n",
        "\n",
        "file_rules:\n",
        "  - !<Rule> {name: rule1, colorspace: cs10, pattern: \"*\", extension: \"*\"}\n",
        "  - !<Rule> {name: Default, colorspace: default}\n",
        "\n",
        "looks:\n",
        "  - !<Look>\n",
        "    name: beauty\n",
        "    process_space: cs5\n",
        "    transform: !<ColorSpaceTransform> {src: cs6, dst: cs6}\n",
        "\n",
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: cs1\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs2\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs3\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs4\n",
        "    from_scene_reference: !<ColorSpaceTransform> {src: cs3, dst: cs3}\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs5\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs6\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs7\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs8\n",
        "    from_scene_reference: !<GroupTransform>\n",
        "      children:\n",
        "        - !<ColorSpaceTransform> {src: cs7, dst: cs7}\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs9\n",
        "    from_scene_reference: !<GroupTransform>\n",
        "      children:\n",
        "        - !<GroupTransform>\n",
        "             children:\n",
        "               - !<LookTransform> {src: cs8, dst: cs8}\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs10\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: cs11\n",
    );

    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    assert!(config.is_color_space_used("cs1")); // Used by a role.
    assert!(config.is_color_space_used("cs2")); // Used by a (display, view) pair.
    assert!(config.is_color_space_used("cs3")); // Used by another color space.
    assert!(config.is_color_space_used("cs5")); // Used by a look i.e. process_space.
    assert!(config.is_color_space_used("cs6")); // Used by a look i.e. ColorSpaceTransform.
    assert!(config.is_color_space_used("cs7")); // Indirectly used by a ColorSpaceTransform.
    assert!(config.is_color_space_used("cs8")); // Indirectly used by a LookTransform.
    assert!(config.is_color_space_used("cs9")); // Used by a inactive (display, view) pair.
    assert!(config.is_color_space_used("cs10")); // Used by a file rule.
    assert!(config.is_color_space_used("cs11")); // Used by a view transform.

    assert!(!config.is_color_space_used("cs4")); // Present but not used.

    // Upstream's null pointer is the empty string.
    assert!(!config.is_color_space_used(""));
    assert!(!config.is_color_space_used(""));
    assert!(!config.is_color_space_used("cs65")); // Unknown color spaces are not used.
}

/// Port of `OCIO_ADD_TEST(Config, view_transforms)` @ v2.5.2.
#[test]
fn view_transforms() {
    use crate::transforms::log_transform::LogTransform;
    use crate::view_transform::ViewTransform;
    use ocio_ops::open_color_types::{ReferenceSpaceType, ViewTransformDirection};

    let _env = EnvGuard::new();
    let str = profile_v2_dcs_start() + SIMPLE_PROFILE_CS_V2;

    let config = Config::create_from_stream(str.as_bytes()).unwrap();
    config.validate().unwrap();

    let mut config_edit = (*config).clone();
    // Create display-referred view transform and add it to the config.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    check_throw_what(
        config_edit.add_view_transform(&vt),
        "Cannot add view transform with an empty name",
    );
    let vt_display = "display";
    vt.set_name(vt_display);
    check_throw_what(
        config_edit.add_view_transform(&vt),
        "Cannot add view transform 'display' with no transform",
    );
    vt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config_edit.add_view_transform(&vt).unwrap();
    assert_eq!(config_edit.num_view_transforms(), 1);
    // Need at least one scene-referred view transform.
    check_throw_what(
        config_edit.validate(),
        "at least one must use the scene reference space",
    );
    assert!(
        config_edit
            .default_scene_to_display_view_transform()
            .is_none()
    );

    // Create scene-referred view transform and add it to the config.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    let vt_scene = "scene";
    vt.set_name(vt_scene);
    vt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config_edit.add_view_transform(&vt).unwrap();
    assert_eq!(config_edit.num_view_transforms(), 2);
    config_edit.validate().unwrap();

    let scene_vt = config_edit
        .default_scene_to_display_view_transform()
        .unwrap()
        .clone();

    assert_eq!(
        vt_display.as_bytes(),
        config_edit.view_transform_name_by_index(0)
    );
    assert_eq!(
        vt_scene.as_bytes(),
        config_edit.view_transform_name_by_index(1)
    );
    assert_eq!(b"", config_edit.view_transform_name_by_index(42));
    assert!(config_edit.view_transform(vt_scene).is_some());
    assert!(config_edit.view_transform("not a view transform").is_none());

    // Default view transform.

    assert_eq!(b"", config_edit.default_view_transform_name());

    config_edit.set_default_view_transform_name("not valid");
    assert_eq!(b"not valid", config_edit.default_view_transform_name());

    check_throw_what(
        config_edit.validate(),
        "Default view transform is defined as: 'not valid' but this does not correspond to an \
         existing scene-referred view transform",
    );

    config_edit.set_default_view_transform_name(vt_display);
    check_throw_what(
        config_edit.validate(),
        "Default view transform is defined as: 'display' but this does not correspond to an \
         existing scene-referred view transform",
    );

    let mut new_scene_vt = scene_vt.clone();
    new_scene_vt.set_name("NotFirst");
    config_edit.add_view_transform(&new_scene_vt).unwrap();

    config_edit.set_default_view_transform_name("NotFirst");
    config_edit.validate().unwrap();

    // Save and reload to test file io for viewTransform.
    let os = config_edit.serialize().unwrap();

    let config_reloaded = Config::create_from_stream(&os).unwrap();
    config_reloaded.validate().unwrap();

    // Setting a view transform with the same name replaces the earlier one.
    vt.set_transform(
        Some(&Transform::from(LogTransform::new())),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config_edit.add_view_transform(&vt).unwrap();
    assert_eq!(config_edit.num_view_transforms(), 3);
    let scene_vt = config_edit.view_transform(vt_scene).unwrap();
    let trans = scene_vt
        .transform(ViewTransformDirection::FromReference)
        .unwrap();
    assert!(matches!(trans, Transform::Log(_)));

    assert_eq!(config_reloaded.num_view_transforms(), 3);

    assert_eq!(b"NotFirst", config_reloaded.default_view_transform_name());

    // Clear all view transforms does not clear the config's default view transform string.

    config_edit.clear_view_transforms();
    assert_eq!(config_edit.num_view_transforms(), 0);

    assert_eq!(b"NotFirst", config_edit.default_view_transform_name());
}

/// Port of `OCIO_ADD_TEST(Config, virtual_display_v2_only)` @ v2.5.2.
#[test]
fn virtual_display_v2_only() {
    use crate::file_rules::FileRules;

    let _env = EnvGuard::new();
    // Test that the virtual display is only supported by v2 or higher.

    const CONFIG: &str = r#"ocio_profile_version: 1

roles:
  default: raw

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

    check_throw_what(
        Config::create_from_stream(CONFIG.as_bytes()),
        "Only version 2 (or higher) can have a virtual display.",
    );

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    cfg.add_virtual_display_shared_view("sview").unwrap();
    cfg.set_major_version(1).unwrap();
    cfg.set_file_rules(&FileRules::new());

    check_throw_what(
        cfg.validate(),
        "Only version 2 (or higher) can have a virtual display.",
    );

    check_throw_what(
        cfg.serialize(),
        "Only version 2 (or higher) can have a virtual display.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, virtual_display_exceptions)` @ v2.5.2.
#[test]
fn virtual_display_exceptions() {
    let _env = EnvGuard::new();
    // Test the validations around the virtual display definition.

    const CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [sview1]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

    let mut cfg = (*Config::create_from_stream(CONFIG.as_bytes()).unwrap()).clone();
    cfg.validate().unwrap();

    // Test failures for shared views.

    check_throw_what(
        cfg.add_virtual_display_shared_view("sview1"),
        "Shared view could not be added to virtual_display: There is already a shared view \
         named 'sview1'.",
    );

    cfg.add_virtual_display_shared_view("sview2").unwrap();
    check_throw_what(
        cfg.validate(),
        "The display 'virtual_display' contains a shared view 'sview2' that is not defined.",
    );

    cfg.remove_virtual_display_view("sview2");
    cfg.validate().unwrap();

    // Test failures for views. (Upstream's null pointers are empty strings.)

    check_throw_what(
        cfg.add_virtual_display_view("Raw", "", "raw", "", "", ""),
        "View could not be added to virtual_display in config: View 'Raw' already exists.",
    );

    cfg.add_virtual_display_view("Raw1", "", "raw1", "", "", "")
        .unwrap();
    check_throw_what(
        cfg.validate(),
        "Display 'virtual_display' has a view 'Raw1' that refers to a color space or a named \
         transform, 'raw1', which is not defined.",
    );

    cfg.remove_virtual_display_view("Raw1");
    cfg.validate().unwrap();

    cfg.add_virtual_display_view("Raw1", "", "raw", "look", "", "")
        .unwrap();
    check_throw_what(
        cfg.validate(),
        "Display 'virtual_display' has a view 'Raw1' refers to a look, 'look', which is not \
         defined.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, remove_color_space)` @ v2.5.2.
#[test]
fn remove_color_space() {
    let _env = EnvGuard::new();
    // The unit test validates that a color space is correctly removed from a configuration.

    let str = profile_v2_start()
        + "    from_scene_reference: !<MatrixTransform> {offset: [-1, -2, -3, -4]}\n"
        + "\n"
        + "  - !<ColorSpace>\n"
        + "    name: cs5\n"
        + "    allocation: uniform\n"
        + "    to_scene_reference: !<FixedFunctionTransform> {style: ACES_RedMod03}\n";

    let mut config = (*Config::create_from_stream(str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();
    assert_eq!(config.num_color_spaces(), 4);

    // Step 1 - Validate the remove.

    assert_eq!(config.index_for_color_space("cs5"), 3);
    config.remove_color_space("cs5");
    assert_eq!(config.num_color_spaces(), 3);
    assert_eq!(config.index_for_color_space("cs5"), -1);

    // Step 2 - Validate some faulty removes.

    // As documented, removing a color space that doesn't exist fails without any notice.
    config.remove_color_space("cs5");
    config.validate().unwrap();

    // Since the method does not support role names, a role name removal fails
    // without any notice except if it's also an existing color space.
    config.remove_color_space("scene_linear");
    config.validate().unwrap();

    // Successfully remove a color space unfortunately used by a role.
    config.remove_color_space("raw");
    // As discussed only validation traps the issue.
    check_throw_what(
        config.validate(),
        "Config failed role validation. The role 'default' refers to a color space, 'raw', \
         which is not defined.",
    );
}

/// Port of `OCIO_ADD_TEST(Config, display_view)` @ v2.5.2.
#[test]
fn display_view() {
    use crate::view_transform::ViewTransform;
    use ocio_ops::open_color_types::{ReferenceSpaceType, ViewTransformDirection};

    let _env = EnvGuard::new();
    // Create a config with a display that has 2 kinds of views.
    let mut config = Config::new().unwrap();
    {
        // Add default color space.
        let mut cs = ColorSpace::new();
        cs.set_name("default");
        cs.set_is_data(true);
        config.add_color_space(&cs).unwrap();
    }

    config.set_version(2, 1).unwrap();

    // Add a scene-referred and a display-referred color space.
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    cs.set_name("scs");
    config.add_color_space(&cs).unwrap();
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs.set_name("dcs");
    config.add_color_space(&cs).unwrap();

    // Add a scene-referred and a display-referred view transform.
    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    vt.set_name("display");
    vt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("view_transform");
    vt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();

    config.set_default_view_transform_name("view_transform");

    // Add a simple view.
    let display = "display";

    assert!(!config.has_view(display, "view1"));

    config
        .add_display_view(display, "view1", "scs", "")
        .unwrap();

    assert!(config.has_view(display, "view1"));

    config.validate().unwrap();

    assert!(!config.has_view(display, "view2"));

    config
        .add_display_view_with_view_transform(display, "view2", "view_transform", "scs", "", "", "")
        .unwrap();
    check_throw_what(
        config.validate(),
        "color space, 'scs', that is not a display-referred",
    );

    assert!(config.has_view(display, "view2"));

    config
        .add_display_view_with_view_transform(display, "view2", "view_transform", "dcs", "", "", "")
        .unwrap();
    assert!(config.has_view(display, "view2"));

    config.validate().unwrap();

    // Validate how the config is serialized.

    let os = config.serialize().unwrap();
    const EXPECTED: &str = r#"ocio_profile_version: 2.1

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  {}

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  display:
    - !<View> {name: view1, colorspace: scs}
    - !<View> {name: view2, view_transform: view_transform, display_colorspace: dcs}

active_displays: []
active_views: []

default_view_transform: view_transform

view_transforms:
  - !<ViewTransform>
    name: display
    from_display_reference: !<MatrixTransform> {}

  - !<ViewTransform>
    name: view_transform
    from_scene_reference: !<MatrixTransform> {}

display_colorspaces:
  - !<ColorSpace>
    name: dcs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

colorspaces:
  - !<ColorSpace>
    name: default
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: scs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
"#;

    check_serialized(&os, EXPECTED);

    let config_read = Config::create_from_stream(&os).unwrap();
    assert_eq!(config_read.num_views("display"), 2);
    let v1 = config_read.view("display", 0).to_vec();
    assert_eq!(v1, b"view1");
    assert_eq!(
        b"scs",
        config_read.display_view_color_space_name("display", &v1)
    );
    assert_eq!(b"", config_read.display_view_transform_name("display", &v1));
    let v2 = config_read.view("display", 1).to_vec();
    assert_eq!(v2, b"view2");
    assert_eq!(
        b"dcs",
        config_read.display_view_color_space_name("display", &v2)
    );
    assert_eq!(
        b"view_transform",
        config_read.display_view_transform_name("display", &v2)
    );
    assert_eq!(b"view_transform", config_read.default_view_transform_name());

    // Check some faulty calls related to displays & views.

    // Using nullptr or empty string for required parameters with throw. (Upstream's null
    // pointers are empty strings, so each of its pairs of checks runs twice here.)
    for _ in 0..2 {
        check_throw_what(
            config.add_display_view("", "view1", "scs", ""),
            "a non-empty display name is needed",
        );
        check_throw_what(
            config.add_display_view(display, "", "scs", ""),
            "a non-empty view name is needed",
        );
        check_throw_what(
            config.add_display_view(display, "view3", "", ""),
            "a non-empty color space name is needed",
        );
        check_throw_what(
            config.add_display_view_with_view_transform(
                display,
                "view4",
                "view_transform",
                "",
                "",
                "",
                "",
            ),
            "a non-empty color space name is needed",
        );
    }
}

/// Port of `OCIO_ADD_TEST(Config, display)` @ v2.5.2.
#[test]
fn display() {
    // EnvGuard::new() unsets the env. variable to make sure the test start in the right
    // environment, as upstream's Platform::Unsetenv; upstream's EnvironmentVariableGuard is
    // env.set(), and the guard's end is the next env.set().
    let env = EnvGuard::new();
    const ACTIVE_DISPLAYS: &str = "OCIO_ACTIVE_DISPLAYS";

    const SIMPLE_PROFILE_HEADER: &str = concat!(
        "ocio_profile_version: 2\n",
        "\n",
        "environment:\n",
        "  {}\n",
        "search_path: luts\n",
        "strictparsing: true\n",
        "luma: [0.2126, 0.7152, 0.0722]\n",
        "\n",
        "roles:\n",
        "  default: raw\n",
        "  scene_linear: lnh\n",
        "\n",
        "file_rules:\n",
        "  - !<Rule> {name: Default, colorspace: default}\n",
        "\n",
        "displays:\n",
        "  sRGB_2:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "  sRGB_F:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "  sRGB_1:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "  sRGB_3:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "  sRGB_B:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "  sRGB_A:\n",
        "    - !<View> {name: Raw, colorspace: raw}\n",
        "\n",
    );

    const SIMPLE_PROFILE_FOOTER: &str = concat!(
        "\n",
        "colorspaces:\n",
        "  - !<ColorSpace>\n",
        "    name: raw\n",
        "    family: \"\"\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    allocation: uniform\n",
        "\n",
        "  - !<ColorSpace>\n",
        "    name: lnh\n",
        "    family: \"\"\n",
        "    equalitygroup: \"\"\n",
        "    bitdepth: unknown\n",
        "    isdata: false\n",
        "    allocation: uniform\n",
    );

    let profile = |active: &str| {
        [
            SIMPLE_PROFILE_HEADER,
            active,
            "active_views: []\n",
            SIMPLE_PROFILE_FOOTER,
        ]
        .concat()
    };

    {
        let my_profile = profile("active_displays: []\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_displays(), 6);
        assert_eq!(config.display(0), b"sRGB_2");
        assert_eq!(config.display(1), b"sRGB_F");
        assert_eq!(config.display(2), b"sRGB_1");
        assert_eq!(config.display(3), b"sRGB_3");
        assert_eq!(config.display(4), b"sRGB_B");
        assert_eq!(config.display(5), b"sRGB_A");
        assert_eq!(config.default_display(), b"sRGB_2");

        check_serialized(&config.serialize().unwrap(), &my_profile);
    }

    {
        let my_profile = profile("active_displays: [sRGB_1]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_displays(), 1);
        assert_eq!(config.display(0), b"sRGB_1");
        assert_eq!(config.default_display(), b"sRGB_1");

        assert_eq!(config.num_displays_all(), 6);

        // Test that all displays are saved.
        check_serialized(&config.serialize().unwrap(), &my_profile);
    }

    {
        let my_profile = profile("active_displays: [sRGB_2, sRGB_1]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();

        assert_eq!(config.num_displays(), 2);
        assert_eq!(config.display(0), b"sRGB_2");
        assert_eq!(config.display(1), b"sRGB_1");
        assert_eq!(config.default_display(), b"sRGB_2");
    }

    {
        let my_profile = profile("active_displays: []\n");

        env.set(&[(ACTIVE_DISPLAYS, " sRGB_3, sRGB_2")]);

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_displays(), 2);
        assert_eq!(config.display(0), b"sRGB_3");
        assert_eq!(config.display(1), b"sRGB_2");
        assert_eq!(config.default_display(), b"sRGB_3");
        env.set(&[]);
    }

    {
        let my_profile = profile("active_displays: [sRGB_2, sRGB_1]\n");

        env.set(&[(ACTIVE_DISPLAYS, " sRGB_3, sRGB_2")]);

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_displays(), 2);
        assert_eq!(config.display(0), b"sRGB_3");
        assert_eq!(config.display(1), b"sRGB_2");
        assert_eq!(config.default_display(), b"sRGB_3");
        env.set(&[]);
    }

    {
        env.set(&[(ACTIVE_DISPLAYS, "")]); // No value

        let my_profile = profile("active_displays: [sRGB_2, sRGB_1]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_displays(), 2);
        assert_eq!(config.display(0), b"sRGB_2");
        assert_eq!(config.display(1), b"sRGB_1");
        assert_eq!(config.default_display(), b"sRGB_2");
        env.set(&[]);
    }

    {
        // No value, but misleading space.
        env.set(&[(ACTIVE_DISPLAYS, " ")]);

        let my_profile = profile("active_displays: [sRGB_2, sRGB_1]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_displays(), 2);
        assert_eq!(config.display(0), b"sRGB_2");
        assert_eq!(config.display(1), b"sRGB_1");
        assert_eq!(config.default_display(), b"sRGB_2");
        env.set(&[]);
    }

    {
        // Test an unknown display name using the env. variable.

        env.set(&[(ACTIVE_DISPLAYS, "ABCDEF")]);

        let my_profile = profile("active_displays: [sRGB_2, sRGB_1]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        check_throw_what(
            config.validate(),
            "The content of the env. variable for the list of active displays [ABCDEF] is \
             invalid.",
        );
        env.set(&[]);
    }

    {
        // Test an unknown display name using the env. variable.

        env.set(&[(ACTIVE_DISPLAYS, "sRGB_2, sRGB_1, ABCDEF")]);

        let my_profile = profile("active_displays: [sRGB_2, sRGB_1]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        check_throw_what(
            config.validate(),
            "The content of the env. variable for the list of active displays [sRGB_2, sRGB_1, \
             ABCDEF] contains invalid display name(s).",
        );
        env.set(&[]);
    }

    {
        // Test an unknown display name in the config active displays.

        env.set(&[]); // Remove the env. variable.

        let my_profile = profile("active_displays: [ABCDEF]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();

        // The active displays list is ignored if it would remove all displays.
        assert_eq!(config.num_displays(), 6);
        assert_eq!(config.display(0), b"sRGB_2");
        assert_eq!(config.display(1), b"sRGB_F");
        assert_eq!(config.default_display(), b"sRGB_2");

        check_throw_what(
            config.validate(),
            "The list of active displays [ABCDEF] from the config file is invalid.",
        );
    }

    {
        // Test an unknown display name in the config active displays.

        env.set(&[]); // Remove the env. variable.

        let my_profile = profile("active_displays: [sRGB_2, sRGB_1, ABCDEF]\n");

        let config = Config::create_from_stream(my_profile.as_bytes()).unwrap();
        check_throw_what(
            config.validate(),
            "The list of active displays [sRGB_2, sRGB_1, ABCDEF] from the config file contains \
             invalid display name(s)",
        );
    }
}

/// Port of `OCIO_ADD_TEST(Config, inactive_color_space_read_write)` @ v2.5.2.
#[test]
fn inactive_color_space_read_write() {
    // The unit tests validate the read/write.

    // EnvGuard::new() unsets OCIO_INACTIVE_COLORSPACES, as upstream's Platform::Unsetenv.
    let env = EnvGuard::new();

    let all = |c: &Config| {
        c.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
    };

    {
        let config_str = [
            INACTIVE_CS_CONFIG_START,
            "inactive_colorspaces: [cs2]\n",
            INACTIVE_CS_CONFIG_END,
        ]
        .concat();

        let config = Config::create_from_stream(config_str.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(all(&config), 5);
        assert_eq!(config.num_color_spaces(), 4);

        check_serialized(&config.serialize().unwrap(), &config_str);
    }

    {
        env.set(&[("OCIO_INACTIVE_COLORSPACES", "cs3, cs1, lnh")]);

        let config_str = [
            INACTIVE_CS_CONFIG_START,
            "inactive_colorspaces: [cs2]\n",
            INACTIVE_CS_CONFIG_END,
        ]
        .concat();

        let config = Config::create_from_stream(config_str.as_bytes()).unwrap();
        {
            // Mute the warnings.
            let (result, _log) = crate::test_env::capture_log(|| config.validate());
            result.unwrap();
        }

        assert_eq!(all(&config), 5);
        assert_eq!(config.num_color_spaces(), 2);

        check_serialized(&config.serialize().unwrap(), &config_str);
        env.set(&[]);
    }

    {
        let config_str = [
            INACTIVE_CS_CONFIG_START,
            // Test a multi-line list.
            "inactive_colorspaces: [cs1\t\n   \n,   \ncs2]\n",
            INACTIVE_CS_CONFIG_END,
        ]
        .concat();

        let config = Config::create_from_stream(config_str.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(all(&config), 5);
        assert_eq!(config.num_color_spaces(), 3);

        let result_str = [
            INACTIVE_CS_CONFIG_START,
            "inactive_colorspaces: [cs1, cs2]\n",
            INACTIVE_CS_CONFIG_END,
        ]
        .concat();

        check_serialized(&config.serialize().unwrap(), &result_str);
    }

    // Do not save an empty 'inactive_colorspaces'.
    {
        let config_str = [
            INACTIVE_CS_CONFIG_START,
            "inactive_colorspaces: []\n",
            INACTIVE_CS_CONFIG_END,
        ]
        .concat();

        let config = Config::create_from_stream(config_str.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(all(&config), 5);
        assert_eq!(config.num_color_spaces(), 5);

        let result_str = [INACTIVE_CS_CONFIG_START, INACTIVE_CS_CONFIG_END].concat();

        check_serialized(&config.serialize().unwrap(), &result_str);
    }

    // Inactive 'unknown' color space ends up to not filter out any color space
    // but still preserved by the read/write.
    {
        let config_str = [
            INACTIVE_CS_CONFIG_START,
            "inactive_colorspaces: [unknown]\n",
            INACTIVE_CS_CONFIG_END,
        ]
        .concat();

        let config = Config::create_from_stream(config_str.as_bytes()).unwrap();

        {
            let (result, log) = crate::test_env::capture_log(|| config.validate());
            result.unwrap();
            assert_eq!(
                String::from_utf8(log.concat()).unwrap(),
                "[OpenColorIO Info]: Inactive 'unknown' is neither a color space nor a named \
                 transform.\n"
            );
        }

        assert_eq!(all(&config), 5);
        assert_eq!(config.num_color_spaces(), 5);

        check_serialized(&config.serialize().unwrap(), &config_str);
    }
}

/// `SIMPLE_PROFILE_B` (Config_tests.cpp:216-227 @ v2.5.2).
const SIMPLE_PROFILE_B: &str = "search_path: luts\n\
strictparsing: true\n\
luma: [0.2126, 0.7152, 0.0722]\n\
\n\
roles:\n  \
aces_interchange: lnh\n  \
color_timing: log\n  \
compositing_log: log\n  \
default: raw\n  \
scene_linear: lnh\n\
\n";

/// `PROFILE_V<Major, Minor>()` (Config_tests.cpp:2175-2190 @ v2.5.2).
fn profile_v(major: u32, minor: u32) -> String {
    let mut s = format!("ocio_profile_version: {major}.{minor}\n");

    if major >= 2 {
        s += "\nenvironment:\n  {}\n";
    }

    s
}

/// `PROFILE_START_V<Major, Minor>()` (Config_tests.cpp:2318-2327 @ v2.5.2).
fn profile_start_v(major: u32, minor: u32) -> String {
    if major <= 1 {
        return profile_v(major, minor)
            + SIMPLE_PROFILE_A
            + SIMPLE_PROFILE_DISPLAYS_LOOKS
            + SIMPLE_PROFILE_CS_V1;
    }

    profile_v(major, minor)
        + SIMPLE_PROFILE_B
        + DEFAULT_RULES
        + SIMPLE_PROFILE_DISPLAYS_LOOKS
        + SIMPLE_PROFILE_CS_V2
}

/// Port of `OCIO_ADD_TEST(Config, interchange_attributes)` @ v2.5.2.
#[test]
fn interchange_attributes() {
    let _env = EnvGuard::new();
    let end = r#"
view_transforms:
  - !<ViewTransform>
    name: vt1
    from_scene_reference: !<RangeTransform> {min_in_value: 0., min_out_value: 0.}"#;

    let str = profile_start_v(2, 5) + end;

    let mut config = (*Config::create_from_stream(str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();

    let contains = |text: &[u8], what: &str| text.windows(what.len()).any(|w| w == what.as_bytes());

    // Color Space

    {
        let mut cs = config.color_space("log").unwrap().clone();

        // Set amf_transform_ids attribute and validate.

        cs.set_interchange_attribute("amf_transform_ids", "sample amf id")
            .unwrap();
        config.add_color_space(&cs).unwrap();
        config.validate().unwrap();

        // Check that the attribute is in the serialized config.

        let ss = config.serialize().unwrap();
        assert!(contains(&ss, "amf_transform_ids: sample amf id"));

        // Check that loading the serialized config works with the attribute.

        let _cfg2 = Config::create_from_stream(&ss).unwrap();

        let cs2 = config.color_space("log").unwrap();
        assert_eq!(
            cs2.interchange_attribute("amf_transform_ids").unwrap(),
            b"sample amf id"
        );

        // Check that the config can NOT be downgraded to 2.4 with the attribute.

        config.set_version(2, 4).unwrap();
        check_throw_what(
            config.validate(),
            "Config failed validation. The color space 'log' has non-empty interchange \
             attributes and config version is less than 2.5.",
        );

        // Remove the attribute and check that the config can be downgraded to 2.4.

        cs.set_interchange_attribute("amf_transform_ids", "")
            .unwrap();
        config.add_color_space(&cs).unwrap();
        config.validate().unwrap();

        // Restore version 2.5.

        config.set_version(2, 5).unwrap();
    }

    // View Transform

    {
        let mut vt = config.view_transform("vt1").unwrap().clone();

        // Set amf_transform_ids attribute and validate.

        vt.set_interchange_attribute("amf_transform_ids", "sample amf id")
            .unwrap();
        config.add_view_transform(&vt).unwrap();
        config.validate().unwrap();

        // Setting the icc_profile_name attribute should throw.
        check_throw_what(
            vt.set_interchange_attribute("icc_profile_name", "some icc profile"),
            "Unknown attribute name 'icc_profile_name'.",
        );

        // Check that the attribute is in the serialized config.

        let ss = config.serialize().unwrap();
        assert!(contains(&ss, "amf_transform_ids: sample amf id"));

        // Check that loading the serialized config works with the attribute.

        let _cfg2 = Config::create_from_stream(&ss).unwrap();

        let vt2 = config.view_transform("vt1").unwrap();
        assert_eq!(
            vt2.interchange_attribute("amf_transform_ids").unwrap(),
            b"sample amf id"
        );

        // Check that the config can NOT be downgraded to 2.4 with the attribute.

        config.set_version(2, 4).unwrap();
        check_throw_what(
            config.validate(),
            "Config failed validation. The view transform 'vt1' has non-empty interchange \
             attributes and config version is less than 2.5.",
        );

        // Remove the attribute and check that the config can be downgraded to 2.4.

        vt.set_interchange_attribute("amf_transform_ids", "")
            .unwrap();
        config.add_view_transform(&vt).unwrap();
        config.validate().unwrap();

        // Restore version 2.5.

        config.set_version(2, 5).unwrap();
    }

    // Look

    {
        let mut lk = config.look("beauty").unwrap().clone();

        // Set amf_transform_ids attribute and validate.

        lk.set_interchange_attribute("amf_transform_ids", "sample amf id")
            .unwrap();
        config.add_look(&lk).unwrap();
        config.validate().unwrap();

        // Check that the attribute is in the serialized config.

        let ss = config.serialize().unwrap();
        assert!(contains(&ss, "amf_transform_ids: sample amf id"));

        // Check that loading the serialized config works with the attribute.

        let _cfg2 = Config::create_from_stream(&ss).unwrap();

        let lk2 = config.look("beauty").unwrap();
        assert_eq!(
            lk2.interchange_attribute("amf_transform_ids").unwrap(),
            b"sample amf id"
        );

        // Check that the config can NOT be downgraded to 2.4 with the attribute.

        config.set_version(2, 4).unwrap();
        check_throw_what(
            config.validate(),
            "Config failed validation. The look 'beauty' has non-empty interchange attributes \
             and config version is less than 2.5.",
        );

        // Remove the attribute and check that the config can be downgraded to 2.4.

        lk.set_interchange_attribute("amf_transform_ids", "")
            .unwrap();
        config.add_look(&lk).unwrap();
        config.validate().unwrap();

        // Restore version 2.5.

        config.set_version(2, 5).unwrap();
    }
}
