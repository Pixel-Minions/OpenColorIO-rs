// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the config. Upstream's `Config version` and `Config family_separator`
//! (tests/cpu/Config_tests.cpp @ v2.5.2) also read and write configs as YAML (Phase 3); their
//! checks of the API alone are here, with upstream's expected messages, without the markers.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::test_env::EnvGuard;
use crate::transforms::matrix_transform::MatrixTransform;

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

/// The views of `Config view` (Config_tests.cpp:3481-3698 @ v2.5.2), but its checks of the
/// serialized config (`operator<<`), which come with the writer (WP 3.7): without its marker
/// until then.
#[test]
fn view_without_serialization() {
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
