// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the named transform. Upstream's `basic` and `alias`
//! (`tests/cpu/NamedTransform_tests.cpp` @ v2.5.2) add named transforms to a config and call
//! `NamedTransform::GetTransform`, so they come with the config's named transforms (WP 3.4j)
//! and the color space transform's builder (WP 3.2a); these tests check the parts that need
//! neither, with upstream's values. The text is compared with the wheel's in
//! `tests/model_objects_oracle.rs`. The `Config` tests that read, write and validate configs
//! without building processors are here too; the others need the builders (WP 3.2d).

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::MatrixTransform;

/// The first part of upstream's `basic` (NamedTransform_tests.cpp:14-46 @ v2.5.2): a new named
/// transform, a copy of its transform, and upstream's text.
#[test]
fn basic_without_a_config() {
    let mut named_transform = NamedTransform::new();
    assert!(named_transform.name().is_empty());
    assert!(
        named_transform
            .transform(TransformDirection::Forward)
            .is_none()
    );
    assert!(
        named_transform
            .transform(TransformDirection::Inverse)
            .is_none()
    );
    let new_name = "NewName";
    named_transform.set_name(new_name);
    assert_eq!(new_name.as_bytes(), named_transform.name());

    let mat: Transform = MatrixTransform::new().into();
    named_transform
        .set_transform(Some(&mat), TransformDirection::Forward)
        .unwrap();
    let fwd_transform = named_transform.transform(TransformDirection::Forward);
    assert!(matches!(fwd_transform, Some(Transform::Matrix(_))));
    assert!(
        named_transform
            .transform(TransformDirection::Inverse)
            .is_none()
    );

    assert_eq!(
        named_transform.to_string(),
        "<NamedTransform name=NewName,\n    forward=\n        <MatrixTransform \
         direction=forward, fileindepth=unknown, fileoutdepth=unknown, matrix=[1, 0, 0, 0, 0, \
         1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], offset=[0, 0, 0, 0]>>"
    );
}

/// The alias part of upstream's `alias` (NamedTransform_tests.cpp:66-138 @ v2.5.2).
#[test]
fn alias_without_a_config() {
    let mut nt = NamedTransform::new();
    assert_eq!(nt.num_aliases(), 0);
    const ALIAS_A: &str = "aliasA";
    const ALIAS_A_ALT: &str = "aLiaSa";
    const ALIAS_B: &str = "aliasB";
    nt.add_alias(ALIAS_A);
    assert_eq!(nt.num_aliases(), 1);
    assert!(nt.has_alias(ALIAS_A));
    assert!(nt.has_alias(ALIAS_A_ALT));
    assert!(!nt.has_alias(ALIAS_B));
    nt.add_alias(ALIAS_B);
    assert_eq!(nt.num_aliases(), 2);
    assert_eq!(nt.alias(0), ALIAS_A.as_bytes());
    assert_eq!(nt.alias(1), ALIAS_B.as_bytes());
    assert!(nt.has_alias(ALIAS_B));

    // Alias with same name (different case) already exists, do nothing.
    nt.add_alias(ALIAS_A_ALT);
    assert_eq!(nt.num_aliases(), 2);
    assert_eq!(nt.alias(0), ALIAS_A.as_bytes());
    assert_eq!(nt.alias(1), ALIAS_B.as_bytes());

    // Remove alias (using a different case).
    nt.remove_alias(ALIAS_A_ALT);
    assert_eq!(nt.num_aliases(), 1);
    assert_eq!(nt.alias(0), ALIAS_B.as_bytes());
    assert!(!nt.has_alias(ALIAS_A));
    assert!(!nt.has_alias(ALIAS_A_ALT));

    // Add with new case.
    nt.add_alias(ALIAS_A_ALT);
    assert_eq!(nt.num_aliases(), 2);
    assert_eq!(nt.alias(0), ALIAS_B.as_bytes());
    assert_eq!(nt.alias(1), ALIAS_A_ALT.as_bytes());
    assert!(nt.has_alias(ALIAS_A));
    assert!(nt.has_alias(ALIAS_A_ALT));

    // Setting the name of the named transform to one of its aliases removes the alias.
    nt.set_name(ALIAS_A);
    assert_eq!(nt.name(), ALIAS_A.as_bytes());
    assert_eq!(nt.num_aliases(), 1);
    assert_eq!(nt.alias(0), ALIAS_B.as_bytes());
    assert!(!nt.has_alias(ALIAS_A));
    assert!(!nt.has_alias(ALIAS_A_ALT));

    // Alias is not added if it is already the named transform name.
    nt.add_alias(ALIAS_A_ALT);
    assert_eq!(nt.name(), ALIAS_A.as_bytes());
    assert_eq!(nt.num_aliases(), 1);
    assert_eq!(nt.alias(0), ALIAS_B.as_bytes());
    assert!(!nt.has_alias(ALIAS_A_ALT));

    // Remove all aliases.
    nt.add_alias("other");
    assert_eq!(nt.num_aliases(), 2);
    assert!(nt.has_alias("other"));
    nt.clear_aliases();
    assert_eq!(nt.num_aliases(), 0);
    assert!(!nt.has_alias(ALIAS_B));
    assert!(!nt.has_alias("other"));
}

// The tests that read, write and validate configs (WP 3.3j, 3.7, 3.8).

/// Port of `OCIO_ADD_TEST(Config, named_transform_io)` @ v2.5.2.
#[test]
fn named_transform_io() {
    let _env = crate::test_env::EnvGuard::new();
    // Validate Config::validate() on config file containing named transforms.

    const OCIO_CONFIG_START: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: raw}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

"#;

    // Test use of named transforms in a role (not allowed), look (not allowed), and file rules
    // (allowed).
    {
        const NT: &str = r#"named_transforms:
  - !<NamedTransform>
    name: namedTransform1
    aliases: [named1, named2]
    family: family
    categories: [input, basic]
    encoding: data
    transform: !<ColorSpaceTransform> {src: default, dst: raw}

  - !<NamedTransform>
    name: namedTransform2
    inverse_transform: !<ColorSpaceTransform> {src: default, dst: raw}
"#;

        let config_str = [OCIO_CONFIG_START, NT].concat();

        let config = crate::Config::create_from_stream(config_str.as_bytes()).unwrap();
        config.validate().unwrap();

        assert_eq!(config.num_named_transforms(), 2);
        assert_eq!(config.named_transform_name_by_index(0), b"namedTransform1");
        assert_eq!(config.named_transform_name_by_index(1), b"namedTransform2");
        let nt = config.named_transform("namedTransform1").unwrap();
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"named1");
        assert_eq!(nt.alias(1), b"named2");
        assert_eq!(nt.family(), b"family");
        assert_eq!(nt.num_categories(), 2);
        assert_eq!(nt.category(0).unwrap(), b"input");
        assert_eq!(nt.category(1).unwrap(), b"basic");
        assert_eq!(nt.encoding(), b"data");
        assert_eq!(config.serialize().unwrap(), config_str.as_bytes());

        // Look can't use named transform.
        let mut look = crate::Look::new();
        look.set_name("look");
        look.set_process_space("namedTransform1");
        let mut config_edit = (*config).clone();
        config_edit.add_look(&look).unwrap();
        check_throw_what(
            config_edit.validate(),
            "process color space, 'namedTransform1', which is not defined",
        );
        config_edit.clear_looks();

        // Role can't use named transform.
        config_edit
            .set_role("newrole", Some(b"namedTransform1"))
            .unwrap();
        check_throw_what(
            config_edit.validate(),
            "refers to a color space, 'namedTransform1', which is not defined",
        );
        config_edit.set_role("newrole", None).unwrap();

        // File rule can use named transform.
        let mut rules = (*config_edit.file_rules().get()).clone();
        rules
            .insert_rule(0, "newrule", "namedTransform1", "*", "*")
            .unwrap();
        config_edit.set_file_rules(&rules);
        config_edit.validate().unwrap();
    }

    // Config can't be read: named transform must define a transform.
    {
        const NT: &str = r#"named_transforms:
  - !<NamedTransform>
    name: namedTransform1"#;

        let config_str = [OCIO_CONFIG_START, NT].concat();

        check_throw_what(
            crate::Config::create_from_stream(config_str.as_bytes()),
            "Named transform must define at least one transform.",
        );
    }

    // Invalid config, named transform holds an invalid transform.
    {
        const NT: &str = r#"named_transforms:
  - !<NamedTransform>
    name: namedTransform1
    transform: !<ColorSpaceTransform> {src: default}
"#;

        let config_str = [OCIO_CONFIG_START, NT].concat();

        let config = crate::Config::create_from_stream(config_str.as_bytes()).unwrap();
        check_throw_what(
            config.validate(),
            "ColorSpaceTransform: empty destination color space name",
        );
    }
}

/// Port of `OCIO_ADD_TEST(Config, colorspace_transform_named_transform)` @ v2.5.2.
#[test]
fn colorspace_transform_named_transform() {
    let _env = crate::test_env::EnvGuard::new();
    // Validate Config::validate() on config with ColorSpace or DisplayView Transforms,
    // or ViewTransforms that reference a Named Transform.

    const OCIO_CONFIG: &str = r#"
ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
  Rec.2100-PQ - Display:
    - !<View> {name: test_view, view_transform: vt, display_colorspace: Rec.2100-PQ - Display}

view_transforms:
  - !<ViewTransform>
    name: vt
    from_scene_reference: !<ColorSpaceTransform> {src: nt, dst: cs2}

display_colorspaces:
  - !<ColorSpace>
    name: Rec.2100-PQ - Display
    isdata: false
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ}

colorspaces:
  - !<ColorSpace>
    name: raw
    isdata: true

  - !<ColorSpace>
    name: cs2
    isdata: false
    from_scene_reference: !<MatrixTransform> {matrix: [ 2.041587903811, -0.565006974279, -0.344731350778, 0, -0.969243636281, 1.875967501508, 0.041555057407, 0, 0.013444280632, -0.118362392231, 1.015174994391, 0, 0, 0, 0, 1 ]}

  - !<ColorSpace>
    name: cs3
    isdata: false
    from_scene_reference: !<ColorSpaceTransform> {src: nt_alias, dst: cs2}

  - !<ColorSpace>
    name: cs4
    isdata: false
    from_scene_reference: !<DisplayViewTransform> {src: nt_alias, display: Rec.2100-PQ - Display, view: test_view}

named_transforms:
  - !<NamedTransform>
    name: nt
    aliases: [nt_alias]
    transform: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [1.49086870465701, -0.268712979082956, -0.222155725704626, 0, -0.0792372106028327, 1.1793685831111, -0.100131372460806, 0, 0.00277810076707935, -0.0304336146315336, 1.02765551391237, 0, 0, 0, 0, 1]}
"#;

    let config = crate::Config::create_from_stream(OCIO_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();
}

/// Upstream's `InactiveNTConfigStart` (NamedTransform_tests.cpp:1303-1325 @ v2.5.2).
const INACTIVE_NT_CONFIG_START: &str = concat!(
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

/// Upstream's `InactiveNTConfigEnd` (NamedTransform_tests.cpp:1327-1368 @ v2.5.2).
const INACTIVE_NT_CONFIG_END: &str = concat!(
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
    "named_transforms:\n",
    "  - !<NamedTransform>\n",
    "    name: nt1\n",
    "    aliases: [alias1]\n",
    "    categories: [cat1]\n",
    "    transform: !<CDLTransform> {offset: [0.1, 0.1, 0.1]}\n",
    "\n",
    "  - !<NamedTransform>\n",
    "    name: nt2\n",
    "    categories: [cat2]\n",
    "    transform: !<CDLTransform> {offset: [0.2, 0.2, 0.2]}\n",
    "\n",
    "  - !<NamedTransform>\n",
    "    name: nt3\n",
    "    categories: [cat3]\n",
    "    transform: !<CDLTransform> {offset: [0.3, 0.3, 0.3]}\n",
);

/// Port of `OCIO_ADD_TEST(Config, inactive_named_transform_precedence)` @ v2.5.2.
#[test]
fn inactive_named_transform_precedence() {
    use ocio_ops::open_color_types::{
        ColorSpaceVisibility, NamedTransformVisibility, SearchReferenceSpaceType,
    };

    let env = crate::test_env::EnvGuard::new();
    // The test demonstrates that an API request supersedes the env. variable and the
    // config file contents.

    let config_str = [
        INACTIVE_NT_CONFIG_START,
        "inactive_colorspaces: [nt2]\n",
        INACTIVE_NT_CONFIG_END,
    ]
    .concat();

    let config = (*crate::Config::create_from_stream(config_str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();

    let nts = |c: &crate::Config, v| c.num_named_transforms_with(v);
    let all_cs = |c: &crate::Config| {
        c.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
    };

    assert_eq!(nts(&config, NamedTransformVisibility::Inactive), 1);
    assert_eq!(nts(&config, NamedTransformVisibility::Active), 2);
    assert_eq!(nts(&config, NamedTransformVisibility::All), 3);
    assert_eq!(all_cs(&config), 2);
    assert_eq!(config.num_color_spaces(), 2);

    assert_eq!(config.named_transform_name_by_index(0), b"nt1");
    assert_eq!(config.named_transform_name_by_index(1), b"nt3");

    // Env. variable supersedes the config content.

    // Upstream's InactiveCSGuard (NamedTransform_tests.cpp:1370-1381).
    env.set(&[("OCIO_INACTIVE_COLORSPACES", "nt3, nt1, lnh")]);

    let mut config = (*crate::Config::create_from_stream(config_str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();

    assert_eq!(nts(&config, NamedTransformVisibility::Inactive), 2);
    assert_eq!(nts(&config, NamedTransformVisibility::Active), 1);
    assert_eq!(nts(&config, NamedTransformVisibility::All), 3);
    assert_eq!(all_cs(&config), 2);
    assert_eq!(config.num_color_spaces(), 1);

    assert_eq!(config.named_transform_name_by_index(0), b"nt2");

    // An API request supersedes the lists from the env. variable and the config file.

    config.set_inactive_color_spaces("nt1, lnh");

    assert_eq!(nts(&config, NamedTransformVisibility::Inactive), 1);
    assert_eq!(nts(&config, NamedTransformVisibility::Active), 2);
    assert_eq!(nts(&config, NamedTransformVisibility::All), 3);
    assert_eq!(all_cs(&config), 2);
    assert_eq!(config.num_color_spaces(), 1);

    assert_eq!(config.named_transform_name_by_index(0), b"nt2");
    assert_eq!(config.named_transform_name_by_index(1), b"nt3");
}
