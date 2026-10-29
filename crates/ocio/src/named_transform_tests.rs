// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the named transform. Upstream's `basic`
//! (`tests/cpu/NamedTransform_tests.cpp` @ v2.5.2) adds named transforms to a config and calls
//! `NamedTransform::GetTransform`, so its complete port also needs the config named transforms
//! and the color space transform's builder (WP 3.2a); these tests check the parts that need
//! neither, with upstream's values. The complete `alias` below also checks its Config assertions.
//! The text is compared with the wheel's in
//! `tests/model_objects_oracle.rs`. Upstream's `Config` tests of named transforms are here too,
//! with the processors the config makes of them (WP 3.2d).

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

/// Port of `OCIO_ADD_TEST(NamedTransform, alias)` @ v2.5.2.
/// Source: tests/cpu/NamedTransform_tests.cpp:64-211; includes all Config assertions.
#[test]
fn alias() {
    let _env = crate::test_env::EnvGuard::new();
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

    // Add and access named transforms in a config.
    let mut config = crate::Config::create_raw().unwrap();
    nt.set_name("name");
    nt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        TransformDirection::Forward,
    )
    .unwrap();

    {
        config.add_named_transform(&nt).unwrap();
        nt.set_name("other");
        nt.add_alias(ALIAS_B);
        config.add_named_transform(&nt).unwrap();
        assert_eq!(config.num_named_transforms(), 2);
        let ntcfg = config.named_transform("name").unwrap();
        assert_eq!(ntcfg.num_aliases(), 0);
    }

    // Access by alias.
    {
        let ntcfg = config.named_transform(ALIAS_B).unwrap();
        assert_eq!(ntcfg.name(), b"other");
        assert_eq!(ntcfg.num_aliases(), 1);
        assert_eq!(config.canonical_name(ALIAS_B), b"other");
        assert_eq!(config.canonical_name("other"), b"other");
        assert_eq!(config.canonical_name("not found"), b"");
        assert_eq!(config.canonical_name(""), b"");
    }

    // Named transform with same name is replaced.
    {
        nt.set_name("name");
        nt.clear_aliases();
        nt.add_alias(ALIAS_A);
        config.add_named_transform(&nt).unwrap();
        assert_eq!(config.num_named_transforms(), 2);
        let ntcfg = config.named_transform("name").unwrap();
        assert_eq!(ntcfg.num_aliases(), 1);
    }

    // Can't add a named transform if name is used as an existing alias.
    {
        nt.set_name(ALIAS_A);
        nt.clear_aliases();
        check_throw_what(
            config.add_named_transform(&nt),
            "Cannot add 'aliasA' named transform, existing named transform, \
             'name' is using this name as an alias",
        );
    }

    // Can't add a named transform if alias is used as an existing alias.
    {
        nt.set_name("newName");
        nt.add_alias(ALIAS_B);
        check_throw_what(
            config.add_named_transform(&nt),
            "Cannot add 'newName' named transform, it has 'aliasB' alias and \
             existing named transform, 'other' is using the same alias",
        );
    }

    // Can't add a named transform if alias is used as an existing name.
    {
        nt.add_alias("other");
        check_throw_what(
            config.add_named_transform(&nt),
            "Cannot add 'newName' named transform, it has 'aliasB' alias and \
             existing named transform, 'other' is using the same alias",
        );
    }
}

/// Port of `OCIO_ADD_TEST(NamedTransform, static_get_transform)` @ v2.5.2.
#[test]
fn static_get_transform() {
    let _env = crate::test_env::EnvGuard::new();
    let config = crate::Config::create_raw().unwrap();

    let offset_f: [f64; 4] = [0.1, 0.2, 0.3, 0.4];
    let offset_i: [f64; 4] = [-offset_f[0], -offset_f[1], -offset_f[2], -offset_f[3]];

    let mut mat1 = MatrixTransform::new();
    mat1.set_offset(&offset_f);
    let mut nt1 = NamedTransform::new();
    nt1.set_transform(Some(&Transform::from(mat1)), TransformDirection::Forward)
        .unwrap();

    let mut mat2 = MatrixTransform::new();
    mat2.set_offset(&offset_i);
    let mut nt2 = NamedTransform::new();
    nt2.set_transform(Some(&Transform::from(mat2)), TransformDirection::Inverse)
        .unwrap();

    // The offsets of the one transform of the processor of `src_tf`.
    let processed_offset = |src_tf: &Transform| {
        let proc = config.processor(src_tf).unwrap();
        let group = proc.create_group_transform().unwrap();
        assert_eq!(group.num_transforms(), 1);
        match group.transform(0).unwrap() {
            Transform::Matrix(mat) => mat.offset(),
            other => panic!("not a MatrixTransform: {other:?}"),
        }
    };

    // Forward transform from forward-only named transform
    {
        let src_tf = NamedTransform::get_transform(&nt1, TransformDirection::Forward).unwrap();
        assert_eq!(processed_offset(&src_tf), offset_f);
    }

    // Inverse transform from forward-only named transform
    {
        let src_tf = NamedTransform::get_transform(&nt1, TransformDirection::Inverse).unwrap();
        assert_eq!(processed_offset(&src_tf), offset_i);
    }

    // Forward transform from inverse-only named transform
    {
        let src_tf = NamedTransform::get_transform(&nt2, TransformDirection::Forward).unwrap();
        assert_eq!(processed_offset(&src_tf), offset_f);
    }

    // Inverse transform from inverse-only named transform
    {
        let src_tf = NamedTransform::get_transform(&nt2, TransformDirection::Inverse).unwrap();
        assert_eq!(processed_offset(&src_tf), offset_i);
    }
}

/// A named transform with neither transform has no transform in either direction, where
/// upstream dereferences null (docs/improvements.md, U-52).
#[test]
fn get_transform_of_a_named_transform_without_transforms() {
    let nt = NamedTransform::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        check_throw_what(
            NamedTransform::get_transform(&nt, dir),
            "Named transform: it has neither a forward nor an inverse transform.",
        );
    }
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

/// Upstream's config of `named_transform_processor` (NamedTransform_tests.cpp:331-388 @ v2.5.2).
const NT_PROCESSOR_CONFIG: &str = r#"ocio_profile_version: 2

search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: ntview, colorspace: ntf}

active_displays: []
active_views: []

display_colorspaces:
  - !<ColorSpace>
    name: dcs
    aliases: [display color space]
    isdata: false
    allocation: uniform
    from_display_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    bitdepth: 32f
    description: |
      A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: cs
    aliases: [colorspace]
    isdata: false
    allocation: uniform
    to_scene_reference: !<RangeTransform> {max_in_value: 1, max_out_value: 1}

named_transforms:
  - !<NamedTransform>
    name: forward
    aliases: [nt1, ntf]
    encoding: scene-linear
    transform: !<MatrixTransform> {name: forward, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: inverse
    aliases: [nt2, nti]
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

  - !<NamedTransform>
    name: both
    aliases: [nt3, ntb]
    transform: !<MatrixTransform> {name: forward, offset: [0.1, 0.2, 0.3, 0.4]}
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;

/// Where upstream reads a matrix's name: the processor's `getTransformFormatMetadata(index)`
/// or the matrix's own `getFormatMetadata()`.
#[derive(Clone, Copy)]
enum NameFrom {
    Processor,
    Matrix,
}

/// Upstream's checks of the transform at `index` of a processor's group: a `MatrixTransform`
/// named `name` whose offset starts with `offset`.
fn check_matrix(
    proc: &crate::Processor,
    group: &crate::GroupTransform,
    index: i32,
    from: NameFrom,
    name: &str,
    offset: [f64; 3],
) {
    let Transform::Matrix(matrix) = group.transform(index).unwrap() else {
        panic!("not a matrix: {:?}", group.transform(index));
    };
    let metadata = match from {
        NameFrom::Processor => proc.transform_format_metadata(index).unwrap(),
        NameFrom::Matrix => matrix.format_metadata(),
    };
    assert_eq!(metadata.get_attribute_value(0), name.as_bytes());
    assert_eq!(matrix.offset()[..3], offset);
}

/// Upstream's checks of a processor whose group holds one matrix (see [`check_matrix`]).
fn check_one_matrix(proc: &crate::Processor, from: NameFrom, name: &str, offset: [f64; 3]) {
    let group = proc.create_group_transform().unwrap();
    assert_eq!(group.num_transforms(), 1);
    check_matrix(proc, &group, 0, from, name, offset);
}

/// Port of `OCIO_ADD_TEST(Config, named_transform_processor)` @ v2.5.2.
#[test]
fn named_transform_processor() {
    use NameFrom::{Matrix, Processor};
    use TransformDirection::{Forward, Inverse};

    let _env = crate::test_env::EnvGuard::new();
    // Create a config with color spaces and named transforms.

    let config = crate::Config::create_from_stream(NT_PROCESSOR_CONFIG.as_bytes()).unwrap();

    let context = config.current_context().get();

    let forward = "forward";
    let offset_f = [0.1, 0.2, 0.3];
    let neg_f = offset_f.map(|v: f64| -v);
    let inverse = "inverse";
    let offset_i = [-0.2, -0.1, -0.1];
    let neg_i = offset_i.map(|v: f64| -v);
    let both = "both";
    let dcs_name = "dcs";
    let cs_name = "cs";

    // Basic named transform access, by name and by alias.
    for name in [forward, "nt1"] {
        let nt = config.named_transform(name).unwrap();
        assert_eq!(nt.encoding(), b"scene-linear");
        // Get the transform in the wanted direction and make sure it exists (else use the other
        // transform in the other direction).
        let tf = nt.transform(Forward).unwrap();

        let proc = config.processor(tf).unwrap();
        check_one_matrix(&proc, Processor, forward, offset_f);
    }

    // Named transform from NamedTransform object and forward direction.
    {
        let nt = config.named_transform("forward");
        assert!(nt.is_some());
        let proc = config.processor_for_named_transform(nt, Forward).unwrap();
        // Named transform forward transform.
        check_one_matrix(&proc, Processor, forward, offset_f);
    }

    // Named transform from NamedTransform object and inverse direction.
    {
        let nt = config.named_transform("forward");
        assert!(nt.is_some());
        let proc = config.processor_for_named_transform(nt, Inverse).unwrap();
        // Named transform inverse transform not available, use forward transform inverted.
        check_one_matrix(&proc, Processor, forward, neg_f);
    }

    // Named transform from NamedTransform object and forward direction with context.
    {
        let nt = config.named_transform("inverse");
        assert!(nt.is_some());
        let proc = config
            .processor_for_named_transform_with_context(&context, nt, Forward)
            .unwrap();
        // Named transform forward transform not available, use inverse transform inverted.
        check_one_matrix(&proc, Processor, inverse, neg_i);
    }

    // Named transform from NamedTransform object and inverse direction with context.
    {
        let nt = config.named_transform("inverse");
        assert!(nt.is_some());
        let proc = config
            .processor_for_named_transform_with_context(&context, nt, Inverse)
            .unwrap();
        // Named transform inverse transform.
        check_one_matrix(&proc, Processor, inverse, offset_i);
    }

    // Named transform from name and forward direction.
    {
        let proc = config
            .processor_for_named_transform_by_name("inverse", Forward)
            .unwrap();
        // Named transform forward transform not available, use inverse transform inverted.
        check_one_matrix(&proc, Processor, inverse, neg_i);
    }

    // Named transform from name and inverse direction.
    {
        let proc = config
            .processor_for_named_transform_by_name("inverse", Inverse)
            .unwrap();
        // Named transform inverse transform.
        check_one_matrix(&proc, Processor, inverse, offset_i);
    }

    // Named transform from name and forward direction with context.
    {
        let proc = config
            .processor_for_named_transform_by_name_with_context(&context, "forward", Forward)
            .unwrap();
        // Named transform forward transform.
        check_one_matrix(&proc, Processor, forward, offset_f);
    }

    // Named transform from name and inverse direction with context.
    {
        let proc = config
            .processor_for_named_transform_by_name_with_context(&context, "forward", Inverse)
            .unwrap();
        // Named transform inverse transform not available, use forward transform inverted.
        check_one_matrix(&proc, Processor, forward, neg_f);
    }

    // Named transform from alias and forward direction.
    {
        let proc = config
            .processor_for_named_transform_by_name("ntb", Forward)
            .unwrap();
        // Named transform has both transforms, use forward transform.
        check_one_matrix(&proc, Processor, forward, offset_f);
    }

    // Named transform from alias and inverse direction.
    {
        let proc = config
            .processor_for_named_transform_by_name("nt3", Inverse)
            .unwrap();
        // Named transform has both transforms, use inverse transform.
        check_one_matrix(&proc, Processor, inverse, offset_i);
    }

    // Diplay color space to named transform, by names and by aliases.
    for (src, dst) in [(dcs_name, forward), ("display color space", "ntf")] {
        let proc = config.processor_between(src, dst).unwrap();
        // Named transform inverse transform not available, use forward transform inverted.
        check_one_matrix(&proc, Processor, forward, neg_f);
    }

    // Color space to named transform, by names and by aliases.
    for (src, dst) in [(cs_name, inverse), ("colorspace", "nt2")] {
        let proc = config.processor_between(src, dst).unwrap();
        // Named transform inverse transform.
        check_one_matrix(&proc, Processor, inverse, offset_i);
    }

    // Diplay color space to named transform (using ColorSpaceTransform).
    {
        let mut cs_transform = crate::ColorSpaceTransform::new();
        cs_transform.set_src(dcs_name);
        cs_transform.set_dst(both);
        let proc = config.processor(&cs_transform.into()).unwrap();
        // Named transform has both transforms, use inverse transform.
        check_one_matrix(&proc, Processor, inverse, offset_i);
    }

    // Named transform to color space, by names and by aliases.
    for (src, dst) in [(forward, cs_name), ("ntf", "colorspace")] {
        let proc = config.processor_between(src, dst).unwrap();
        // Named transform forward transform.
        check_one_matrix(&proc, Matrix, forward, offset_f);
    }

    // Named transform to display color space.
    {
        let proc = config.processor_between(inverse, dcs_name).unwrap();
        // Named transform forward transform not available, use inverse transform inverted.
        check_one_matrix(&proc, Matrix, inverse, neg_i);
    }

    // Named transform to color space.
    {
        let proc = config.processor_between(both, cs_name).unwrap();
        // Named transform has both transforms, use forward transform.
        check_one_matrix(&proc, Matrix, forward, offset_f);
    }

    // Named transform to named transform, by names and by aliases.
    for (src, dst) in [(both, both), ("nt3", "ntb")] {
        let proc = config.processor_between(src, dst).unwrap();
        let group = proc.create_group_transform().unwrap();
        assert_eq!(group.num_transforms(), 2);
        // Named transform forward transform.
        check_matrix(&proc, &group, 0, Processor, forward, offset_f);
        // Named transform inverse transform.
        check_matrix(&proc, &group, 1, Processor, inverse, offset_i);
    }

    {
        let proc = config
            .processor_for_display_view("colorspace", "sRGB", "ntview", Forward)
            .unwrap();
        // Named transform has both transforms, use forward transform.
        check_one_matrix(&proc, Matrix, forward, offset_f);
    }

    // See DisplayViewTransform_tests.cpp for additional named transform tests related to their
    // use in displays/views.
}

/// Port of `OCIO_ADD_TEST(Config, inactive_named_transforms)` @ v2.5.2.
#[test]
fn inactive_named_transforms() {
    use NamedTransformVisibility as Nt;
    use ocio_ops::open_color_types::{
        ColorSpaceVisibility, NamedTransformVisibility, SearchReferenceSpaceType,
    };

    let _env = crate::test_env::EnvGuard::new();
    // The unit test validates the inactive named transforms behavior.
    // Tests (Config, inactive_color_space) and (Config, inactive_color_space_precedence) from
    // Config_tests.cpp are testing inactive color spaces without any named transforms.

    let config_str = [INACTIVE_NT_CONFIG_START, INACTIVE_NT_CONFIG_END].concat();

    let mut config = (*crate::Config::create_from_stream(config_str.as_bytes()).unwrap()).clone();
    config.validate().unwrap();

    let nts = |c: &crate::Config, v| c.num_named_transforms_with(v);
    let nt_name = |c: &crate::Config, v, i| c.named_transform_name_by_index_with(v, i).to_vec();
    let css = |c: &crate::Config, v| c.num_color_spaces_with(SearchReferenceSpaceType::All, v);

    // Step 1 - No inactive named transforms.

    assert_eq!(nts(&config, Nt::Inactive), 0);
    assert_eq!(nts(&config, Nt::Active), 3);
    assert_eq!(nts(&config, Nt::All), 3);

    assert_eq!(nt_name(&config, Nt::All, 0), b"nt1");
    assert_eq!(nt_name(&config, Nt::All, 1), b"nt2");
    assert_eq!(nt_name(&config, Nt::All, 2), b"nt3");
    // Check a faulty call.
    assert_eq!(nt_name(&config, Nt::All, 3), b"");

    assert_eq!(config.num_named_transforms(), 3);
    assert_eq!(config.named_transform_name_by_index(0), b"nt1");
    assert_eq!(config.named_transform_name_by_index(1), b"nt2");
    assert_eq!(config.named_transform_name_by_index(2), b"nt3");
    // Check a faulty call.
    assert_eq!(config.named_transform_name_by_index(3), b"");

    // Step 2 - Some inactive color space and named transforms (aliases can be used).

    config.set_inactive_color_spaces("lnh, alias1");
    assert_eq!(config.inactive_color_spaces(), b"lnh, alias1");

    assert_eq!(css(&config, ColorSpaceVisibility::Inactive), 1);
    assert_eq!(css(&config, ColorSpaceVisibility::Active), 1);
    assert_eq!(css(&config, ColorSpaceVisibility::All), 2);

    assert_eq!(nts(&config, Nt::Inactive), 1);
    assert_eq!(nts(&config, Nt::Active), 2);
    assert_eq!(nts(&config, Nt::All), 3);

    // Check methods working with activity flag.
    assert_eq!(nt_name(&config, Nt::All, 0), b"nt1");
    assert_eq!(nt_name(&config, Nt::All, 1), b"nt2");
    assert_eq!(nt_name(&config, Nt::All, 2), b"nt3");
    assert_eq!(nt_name(&config, Nt::Active, 0), b"nt2");
    assert_eq!(nt_name(&config, Nt::Active, 1), b"nt3");
    assert_eq!(nt_name(&config, Nt::Inactive, 0), b"nt1");

    // Check methods working on only active named transforms.
    assert_eq!(config.num_named_transforms(), 2);
    assert_eq!(config.named_transform_name_by_index(0), b"nt2");
    assert_eq!(config.named_transform_name_by_index(1), b"nt3");

    // Request an active named transform.
    let nt = config.named_transform("nt2").unwrap();
    assert_eq!(nt.name(), b"nt2");

    // Request an inactive named transform.
    let nt = config.named_transform("nt1").unwrap();
    assert_eq!(nt.name(), b"nt1");
    assert_eq!(config.index_for_named_transform(nt.name()), -1);

    // Request an inactive named transform using alias.
    let nt = config.named_transform("alias1").unwrap();
    assert_eq!(nt.name(), b"nt1");
    assert_eq!(config.index_for_named_transform(nt.name()), -1);

    // Create a processor with one or more inactive color spaces or named transforms.
    config.processor_between("lnh", "nt1").unwrap();
    config.processor_between("raw", "nt1").unwrap();
    config.processor_between("lnh", "nt2").unwrap();
    config.processor_between("nt2", "scene_linear").unwrap();

    // Step 3 - No inactive color spaces or named transforms.

    config.set_inactive_color_spaces("");
    assert_eq!(config.inactive_color_spaces(), b"");

    assert_eq!(css(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(nts(&config, Nt::All), 3);
    assert_eq!(config.num_named_transforms(), 3);

    // Step 4 - No inactive color spaces or named transforms can also use nullptr.

    config.set_inactive_color_spaces("lnh, nt1");
    assert_eq!(config.inactive_color_spaces(), b"lnh, nt1");

    // Upstream's nullptr: the port's setter takes text, and null is the empty text there.
    config.set_inactive_color_spaces("");
    assert_eq!(config.inactive_color_spaces(), b"");

    assert_eq!(css(&config, ColorSpaceVisibility::All), 2);
    assert_eq!(config.num_color_spaces(), 2);
    assert_eq!(nts(&config, Nt::All), 3);
    assert_eq!(config.num_named_transforms(), 3);
}

/// Port of `OCIO_ADD_TEST(Config, named_transform_validation)` @ v2.5.2.
#[test]
fn named_transform_validation() {
    use ocio_ops::open_color_types::{ReferenceSpaceType, ViewTransformDirection};

    let _env = crate::test_env::EnvGuard::new();
    let mut config = (*crate::Config::create_raw().unwrap()).clone();

    let mut named_transform = NamedTransform::new();
    let name1 = "name";
    named_transform.set_name(name1);
    let mut mat = MatrixTransform::new();
    let offset = [0.1, 0.2, 0.3, 0.4];
    mat.set_offset(&offset);
    let mat: Transform = mat.into();
    named_transform
        .set_transform(Some(&mat), TransformDirection::Forward)
        .unwrap();

    config.add_named_transform(&named_transform).unwrap();
    assert_eq!(config.num_named_transforms(), 1);

    let name2 = "other_name";
    named_transform.set_name(name2);
    named_transform
        .set_transform(Some(&mat), TransformDirection::Inverse)
        .unwrap();
    config.add_named_transform(&named_transform).unwrap();
    assert_eq!(config.num_named_transforms(), 2);

    config.validate().unwrap();

    assert_eq!(name1.as_bytes(), config.named_transform_name_by_index(0));
    assert_eq!(name2.as_bytes(), config.named_transform_name_by_index(1));
    assert_eq!(b"", config.named_transform_name_by_index(2));

    let nt = config.named_transform(name1).unwrap();
    assert!(nt.transform(TransformDirection::Forward).is_some());
    assert!(nt.transform(TransformDirection::Inverse).is_none());

    let nt = config.named_transform(name2).unwrap();
    assert!(nt.transform(TransformDirection::Forward).is_some());
    assert!(nt.transform(TransformDirection::Inverse).is_some());

    config.processor_between("raw", name1).unwrap();

    config.processor_between(name1, name1).unwrap();

    check_throw_what(
        config.processor_between("raw", "missing"),
        "Color space 'missing' could not be found",
    );

    // NamedTransform can't use a role name.
    check_throw_what(
        config.set_role(name1, Some(b"raw")),
        "Cannot add 'name' role, there is already a named transform using this as a name or \
         an alias",
    );
    config.set_role(name1, None).unwrap();

    // NamedTransform can't use a color space name.
    let mut cs = crate::ColorSpace::new();
    cs.set_name(name1);
    check_throw_what(
        config.add_color_space(&cs),
        "Cannot add 'name' color space, there is already a named transform using this name as \
         a name or as an alias: 'name'",
    );
    config.remove_color_space(name1);

    // NamedTransform can't use a look name.
    let mut look = crate::Look::new();
    look.set_name(name1);
    look.set_process_space("raw");

    config.add_look(&look).unwrap();
    check_throw_what(config.validate(), "This name is already used for a look");
    config.clear_looks();

    // NamedTransform can't use a view transform name.
    let mut vt = crate::ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name(name1);
    vt.set_transform(
        Some(&MatrixTransform::new().into()),
        ViewTransformDirection::ToReference,
    )
    .unwrap();

    config.add_view_transform(&vt).unwrap();
    check_throw_what(
        config.validate(),
        "This name is already used for a view transform",
    );
    config.clear_view_transforms();

    config.set_major_version(1).unwrap();
    config.set_file_rules(&crate::FileRules::new());
    check_throw_what(
        config.validate(),
        "Only version 2 (or higher) can have NamedTransforms",
    );
}
