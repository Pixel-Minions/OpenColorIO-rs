// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the config (tests/cpu/Config_tests.cpp @ v2.5.2): those that read configs (group
//! A), and those that write them without validating them (group C, WP 3.7). Upstream's `Config
//! version` and `Config family_separator` also validate configs (WP 3.8); their checks of the
//! API alone are here, with upstream's expected messages, without the markers.

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

/// `Config version`'s checks of `setMinorVersion` and `setVersion` (Config_tests.cpp:2065-2107
/// @ v2.5.2), on a new config instead of one read from YAML.
#[test]
fn version_without_yaml() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();

    config.set_major_version(1).unwrap();
    check_throw_what(
        config.set_major_version(20000),
        "version is 20000 where supported versions start at 1 and end at 2",
    );

    check_throw_what(
        config.set_minor_version(1),
        "The minor version 1 is not supported for major version 1. Maximum minor version is 0",
    );

    config.set_minor_version(0).unwrap();
    config.set_major_version(2).unwrap();

    check_throw_what(
        config.set_version(2, 9),
        "The minor version 9 is not supported for major version 2. Maximum minor version is 5",
    );

    config.set_major_version(2).unwrap();
    check_throw_what(
        config.set_minor_version(9),
        "The minor version 9 is not supported for major version 2. Maximum minor version is 5",
    );

    check_throw_what(
        config.set_version(3, 4),
        "version is 3 where supported versions start at 1 and end at 2",
    );
}

/// `Config family_separator`'s checks of the API (Config_tests.cpp:7486-7500 @ v2.5.2), on a
/// copy of the raw config.
#[test]
fn family_separator_without_yaml() {
    let _env = EnvGuard::new();
    let mut cfg = (*Config::create_raw().unwrap()).clone();

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
}

/// `Config alias_validation`'s checks (Config_tests.cpp:9411-9478 @ v2.5.2), without its
/// `validate()` calls (3.8a; no marker).
#[test]
fn alias_validation_without_validate() {
    let _env = EnvGuard::new();
    // NB: This tests ColorSpaceSet::addColorSpace.

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::new();
    cs.set_name("colorspace1");
    cfg.add_color_space(&cs).unwrap();
    cs.set_name("colorspace2");
    cfg.add_color_space(&cs).unwrap();
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

/// The two configs of `Config compare_displays` (Display_tests.cpp:242-326 @ v2.5.2), built
/// through the API as the YAML reader builds them (without their roles, file rules and view
/// transforms, which these checks never read).
fn compare_displays_configs() -> (Config, Config) {
    let mut config1 = Config::new().unwrap();
    for (name, display) in [(&b"raw"[..], false), (b"display_cs", true)] {
        let mut cs = if display {
            ColorSpace::with_reference_space(ReferenceSpaceType::Display)
        } else {
            ColorSpace::new()
        };
        cs.set_name(name);
        config1.add_color_space(&cs).unwrap();
    }
    let mut config2 = config1.clone();

    config1
        .add_shared_view("sview1", "", "raw", "", "", "")
        .unwrap();
    config1.add_display_view("Raw", "Raw", "raw", "").unwrap();
    config1.add_display_view("sRGB", "Raw", "raw", "").unwrap();
    config1
        .add_display_view_with_view_transform(
            "sRGB",
            "view",
            "display_vt",
            "display_cs",
            "",
            "",
            "",
        )
        .unwrap();
    config1.add_display_shared_view("sRGB", "sview1").unwrap();
    config1.set_active_displays("sRGB").unwrap();
    config1.set_active_views("view, sview1").unwrap();

    config2
        .add_shared_view("view", "display_vt", "display_cs", "", "", "")
        .unwrap();
    config2
        .add_shared_view("sview1", "", "raw", "", "", "")
        .unwrap();
    config2.add_display_view("Raw", "Raw", "raw", "").unwrap();
    config2.add_display_view("sRGB", "Raw", "raw", "").unwrap();
    config2.add_display_shared_view("sRGB", "view").unwrap();
    config2.add_display_shared_view("sRGB", "sview1").unwrap();
    config2.set_active_displays("Raw").unwrap();
    config2.set_active_views("Raw").unwrap();
    (config1, config2)
}

/// `Config compare_displays`'s checks (Display_tests.cpp:328-515 @ v2.5.2) on configs built
/// through the API instead of read from YAML, without its `validate()` calls (no marker: the
/// test reads YAML).
#[test]
fn compare_displays_without_yaml() {
    let _env = EnvGuard::new();
    let (config1, config2) = compare_displays_configs();

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
