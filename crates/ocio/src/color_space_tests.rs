// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the color space: `tests/cpu/ColorSpace_tests.cpp` @ v2.5.2, with those that read,
//! write and validate configs (WP 3.3j, 3.7b, 3.8). The processor ones come with Phase 2 and
//! ConfigUtils (Phase 9). The text is compared with the wheel's in
//! `tests/model_objects_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(ColorSpace, basic)` @ v2.5.2.
#[test]
fn basic() {
    let cs = ColorSpace::new();
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);

    let cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Display);

    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
    assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);

    assert_eq!(b"", cs.name());
    assert_eq!(0, cs.num_aliases());
    assert_eq!(b"", cs.alias(0));
    assert_eq!(b"", cs.family());
    assert_eq!(b"", cs.description());
    assert_eq!(b"", cs.equality_group());
    assert_eq!(b"", cs.encoding());
    assert_eq!(BitDepth::Unknown, cs.bit_depth());
    assert!(!cs.is_data());
    assert_eq!(Allocation::Uniform, cs.allocation());
    assert_eq!(0, cs.allocation_num_vars());

    // Check the nullptr assignment hardening.
    // First set the fields to non-empty values.
    cs.set_name("NAME");
    cs.set_description("DESC");
    cs.set_family("FAMILY");
    cs.set_equality_group("EQGRP");
    cs.set_encoding("ENC");
    cs.set_interop_id("interop").unwrap();
    assert!(
        cs.set_interchange_attribute("amf_transform_ids", "AMF")
            .is_ok()
    );
    assert!(
        cs.set_interchange_attribute("icc_profile_name", "ICC")
            .is_ok()
    );

    // Set to nullptr, this should erase the old values. (A null pointer is an empty slice
    // here.)
    cs.set_name("");
    cs.set_description("");
    cs.set_family("");
    cs.set_equality_group("");
    cs.set_encoding("");
    assert!(cs.set_interop_id("").is_ok());
    assert!(
        cs.set_interchange_attribute("amf_transform_ids", "")
            .is_ok()
    );
    assert!(cs.set_interchange_attribute("icc_profile_name", "").is_ok());

    // Check that the values are empty now.
    assert!(cs.name().is_empty());
    assert!(cs.description().is_empty());
    assert!(cs.family().is_empty());
    assert!(cs.equality_group().is_empty());
    assert!(cs.encoding().is_empty());
    assert!(cs.interop_id().is_empty());
    assert!(
        cs.interchange_attribute("amf_transform_ids")
            .unwrap()
            .is_empty()
    );
    assert!(
        cs.interchange_attribute("icc_profile_name")
            .unwrap()
            .is_empty()
    );

    // Test set/get roundtrip.
    cs.set_name("name");
    assert_eq!(b"name", cs.name());
    cs.set_family("family");
    assert_eq!(b"family", cs.family());
    cs.set_description("description");
    assert_eq!(b"description", cs.description());
    cs.set_equality_group("equalitygroup");
    assert_eq!(b"equalitygroup", cs.equality_group());
    cs.set_encoding("encoding");
    assert_eq!(b"encoding", cs.encoding());
    cs.set_bit_depth(BitDepth::F16);
    assert_eq!(BitDepth::F16, cs.bit_depth());
    cs.set_is_data(true);
    assert!(cs.is_data());
    cs.set_allocation(Allocation::Unknown);
    assert_eq!(Allocation::Unknown, cs.allocation());
    let vars = [1.0f32, 2.0];
    cs.set_allocation_vars(&vars);
    assert_eq!(2, cs.allocation_num_vars());
    let read_vars = cs.allocation_vars();
    assert_eq!(1.0, read_vars[0]);
    assert_eq!(2.0, read_vars[1]);
    cs.set_interop_id("interop_id").unwrap();
    assert_eq!(b"interop_id", cs.interop_id());
    cs.set_interchange_attribute("amf_transform_ids", "amf_transform_id1\namf_transform_id2")
        .unwrap();
    assert_eq!(
        b"amf_transform_id1\namf_transform_id2",
        cs.interchange_attribute("amf_transform_ids").unwrap()
    );
    cs.set_interchange_attribute("icc_profile_name", "icc_profile_name")
        .unwrap();
    assert_eq!(
        b"icc_profile_name",
        cs.interchange_attribute("icc_profile_name").unwrap()
    );

    let oss = cs.to_string();
    assert_eq!(oss.len(), 306);
}

/// Port of `OCIO_ADD_TEST(ColorSpace, alias)` @ v2.5.2.
#[test]
fn alias() {
    let mut cs = ColorSpace::new();
    assert_eq!(cs.num_aliases(), 0);
    const ALIAS_A: &str = "aliasA";
    const ALIAS_A_ALT: &str = "aLiaSa";
    const ALIAS_B: &str = "aliasB";
    cs.add_alias(ALIAS_A);
    assert_eq!(cs.num_aliases(), 1);
    assert!(cs.has_alias(ALIAS_A));
    assert!(cs.has_alias(ALIAS_A_ALT));
    assert!(!cs.has_alias(ALIAS_B));
    cs.add_alias(ALIAS_B);
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), ALIAS_A.as_bytes());
    assert_eq!(cs.alias(1), ALIAS_B.as_bytes());
    assert!(cs.has_alias(ALIAS_B));

    // Alias with same name (different case) already exists, do nothing.

    cs.add_alias(ALIAS_A_ALT);
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), ALIAS_A.as_bytes());
    assert_eq!(cs.alias(1), ALIAS_B.as_bytes());

    // Remove alias.

    cs.remove_alias(ALIAS_A_ALT);
    assert_eq!(cs.num_aliases(), 1);
    assert_eq!(cs.alias(0), ALIAS_B.as_bytes());
    assert!(!cs.has_alias(ALIAS_A));
    assert!(!cs.has_alias(ALIAS_A_ALT));

    // Add with new case.

    cs.add_alias(ALIAS_A_ALT);
    assert_eq!(cs.num_aliases(), 2);
    assert_eq!(cs.alias(0), ALIAS_B.as_bytes());
    assert_eq!(cs.alias(1), ALIAS_A_ALT.as_bytes());
    assert!(cs.has_alias(ALIAS_A));
    assert!(cs.has_alias(ALIAS_A_ALT));

    // Setting the name of the color space to one of its aliases removes the alias.

    cs.set_name(ALIAS_A);
    assert_eq!(cs.name(), ALIAS_A.as_bytes());
    assert_eq!(cs.num_aliases(), 1);
    assert_eq!(cs.alias(0), ALIAS_B.as_bytes());
    assert!(!cs.has_alias(ALIAS_A));
    assert!(!cs.has_alias(ALIAS_A_ALT));

    // Alias is not added if it is already the color space name.

    cs.add_alias(ALIAS_A_ALT);
    assert_eq!(cs.name(), ALIAS_A.as_bytes());
    assert_eq!(cs.num_aliases(), 1);
    assert_eq!(cs.alias(0), ALIAS_B.as_bytes());
    assert!(!cs.has_alias(ALIAS_A_ALT));

    // Remove all aliases.

    cs.add_alias("other");
    assert_eq!(cs.num_aliases(), 2);
    assert!(cs.has_alias("other"));
    cs.clear_aliases();
    assert_eq!(cs.num_aliases(), 0);
    assert!(!cs.has_alias(ALIAS_B));
    assert!(!cs.has_alias("other"));
}

/// Port of `OCIO_ADD_TEST(ColorSpace, category)` @ v2.5.2.
#[test]
fn category() {
    let mut cs = ColorSpace::new();
    assert_eq!(cs.num_categories(), 0);

    assert!(!cs.has_category("linear"));
    assert!(!cs.has_category("rendering"));
    assert!(!cs.has_category("log"));

    cs.add_category("linear");
    cs.add_category("rendering");
    assert_eq!(cs.num_categories(), 2);

    assert!(cs.has_category("linear"));
    assert!(cs.has_category("rendering"));
    assert!(!cs.has_category("log"));

    assert_eq!(cs.category(0), Some(&b"linear"[..]));
    assert_eq!(cs.category(1), Some(&b"rendering"[..]));
    // Check with an invalid index.
    assert!(cs.category(2).is_none());

    cs.remove_category("linear");
    assert_eq!(cs.num_categories(), 1);
    assert!(!cs.has_category("linear"));
    assert!(cs.has_category("rendering"));
    assert!(!cs.has_category("log"));

    // Remove a category not in the color space.
    cs.remove_category("log");
    assert_eq!(cs.num_categories(), 1);
    assert!(cs.has_category("rendering"));

    cs.clear_categories();
    assert_eq!(cs.num_categories(), 0);
}

/// Port of `OCIO_ADD_TEST(ColorSpace, interop_id)` @ v2.5.2.
#[test]
fn interop_id() {
    let mut cs = ColorSpace::new();

    // Test default value.
    assert_eq!(cs.interop_id(), b"");

    // Test setting and getting single profile name.
    let interop_id = "srgb_p3d65_scene";
    cs.set_interop_id(interop_id).unwrap();
    assert_eq!(cs.interop_id(), interop_id.as_bytes());

    // Test setting empty string.
    cs.set_interop_id("").unwrap();
    assert_eq!(cs.interop_id(), b"");

    // Test setting and getting another value.
    let another_id = "lin_rec2020_scene";
    cs.set_interop_id(another_id).unwrap();
    assert_eq!(cs.interop_id(), another_id.as_bytes());

    // Test setting null pointer (should be safe). (A null pointer is an empty slice here.)
    cs.set_interop_id("something").unwrap();
    assert!(cs.set_interop_id("").is_ok());
    assert_eq!(cs.interop_id(), b"");

    // Test copy constructor preserves InteropID.
    cs.set_interop_id(interop_id).unwrap();
    let copy = cs.clone();
    assert_eq!(copy.interop_id(), interop_id.as_bytes());

    // Test valid InteropID with colon in the middle.
    let valid_colon_middle = "namespace:colorspace_name";
    assert!(cs.set_interop_id(valid_colon_middle).is_ok());
    assert_eq!(cs.interop_id(), valid_colon_middle.as_bytes());

    // Test invalid InteropID with multiple colons.
    check_throw_what(
        cs.set_interop_id("name:space:cs_name"),
        "Only one ':' is allowed to separate the namespace and the color space.",
    );

    // Test invalid InteropID with colon at the end.
    check_throw_what(
        cs.set_interop_id("namespace:"),
        " If ':' is used, both the namespace and the color space parts must be non-empty.",
    );

    // Test invalid InteropID with empty namespace or color space.
    check_throw_what(
        cs.set_interop_id(":cs_name"),
        "If ':' is used, both the namespace and the color space parts must be non-empty.",
    );

    // Test invalid InteropID with illegal characters.
    check_throw_what(
        cs.set_interop_id("caf\u{e9}_scene"),
        "Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.",
    );

    check_throw_what(
        cs.set_interop_id("UPPERCASE"),
        "Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.",
    );

    check_throw_what(
        cs.set_interop_id("{curly_bracket}"),
        "Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.",
    );

    check_throw_what(
        cs.set_interop_id("\\backslash"),
        "Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.",
    );

    check_throw_what(
        cs.set_interop_id(" space "),
        "Only lowercase a-z, 0-9 and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.",
    );
}

/// Port of `OCIO_ADD_TEST(ColorSpace, amf_transform_ids)` @ v2.5.2.
#[test]
fn amf_transform_ids() {
    let mut cs = ColorSpace::new();

    // Test default value.
    assert_eq!(cs.interchange_attribute("amf_transform_ids").unwrap(), b"");

    // Test setting and getting single ID.
    let single_id = "urn:ampas:aces:transformId:v1.5:ACEScsc.Academy.ACEScc_to_ACES.a1.0.3";
    cs.set_interchange_attribute("amf_transform_ids", single_id)
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("amf_transform_ids").unwrap(),
        single_id.as_bytes()
    );

    // Test setting to empty string.
    cs.set_interchange_attribute("amf_transform_ids", "")
        .unwrap();
    assert_eq!(cs.interchange_attribute("amf_transform_ids").unwrap(), b"");

    // Test setting and getting multiple IDs.
    let multiple_ids = "urn:ampas:aces:transformId:v1.5:ACEScsc.Academy.ACEScc_to_ACES.a1.0.3\n\
                        urn:ampas:aces:transformId:v1.5:ACEScsc.Academy.ACES_to_ACEScc.a1.0.3";
    cs.set_interchange_attribute("amf_transform_ids", multiple_ids)
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("amf_transform_ids").unwrap(),
        multiple_ids.as_bytes()
    );

    // Test setting to null pointer (should be safe). (A null pointer is an empty slice here.)
    cs.set_interchange_attribute("amf_transform_ids", "something")
        .unwrap();
    cs.set_interchange_attribute("amf_transform_ids", "")
        .unwrap();
    assert_eq!(cs.interchange_attribute("amf_transform_ids").unwrap(), b"");

    // Test copy constructor preserves AMF transform IDs.
    cs.set_interchange_attribute("amf_transform_ids", single_id)
        .unwrap();
    let copy = cs.clone();
    assert_eq!(
        copy.interchange_attribute("amf_transform_ids").unwrap(),
        single_id.as_bytes()
    );
}

/// Port of `OCIO_ADD_TEST(ColorSpace, icc_profile_name)` @ v2.5.2.
#[test]
fn icc_profile_name() {
    let mut cs = ColorSpace::new();

    // Test default value.
    assert_eq!(cs.interchange_attribute("icc_profile_name").unwrap(), b"");

    // Test setting and getting single profile name.
    let profile_name = "sRGB IEC61966-2.1";
    cs.set_interchange_attribute("icc_profile_name", profile_name)
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("icc_profile_name").unwrap(),
        profile_name.as_bytes()
    );

    // Test setting and getting another profile name.
    let another_profile = "Adobe RGB (1998)";
    cs.set_interchange_attribute("icc_profile_name", another_profile)
        .unwrap();
    assert_eq!(
        cs.interchange_attribute("icc_profile_name").unwrap(),
        another_profile.as_bytes()
    );

    // Test setting empty string.
    cs.set_interchange_attribute("icc_profile_name", "")
        .unwrap();
    assert_eq!(cs.interchange_attribute("icc_profile_name").unwrap(), b"");

    // Test setting null pointer (should be safe). (A null pointer is an empty slice here.)
    cs.set_interchange_attribute("icc_profile_name", "something")
        .unwrap();
    assert!(cs.set_interchange_attribute("icc_profile_name", "").is_ok());
    assert_eq!(cs.interchange_attribute("icc_profile_name").unwrap(), b"");

    // Test copy constructor preserves ICC profile name.
    cs.set_interchange_attribute("icc_profile_name", profile_name)
        .unwrap();
    let copy = cs.clone();
    assert_eq!(
        copy.interchange_attribute("icc_profile_name").unwrap(),
        profile_name.as_bytes()
    );
}

/// Port of `OCIO_ADD_TEST(ColorSpace, unknown_interchange_attrib)` @ v2.5.2.
#[test]
fn unknown_interchange_attrib() {
    let mut cs = ColorSpace::new();

    // Getting should throw.
    check_throw_what(
        cs.interchange_attribute("unknown_attrib"),
        "Unknown attribute name",
    );

    // Empty name is not legal. (A null pointer is an empty slice here.)
    check_throw_what(cs.interchange_attribute(""), "Unknown attribute name");
    check_throw_what(cs.interchange_attribute(b""), "Unknown attribute name");

    // Setting should throw too.
    check_throw_what(
        cs.set_interchange_attribute("unknown_attribute1", "unknown"),
        "Unknown attribute name",
    );
    check_throw_what(
        cs.set_interchange_attribute("unknown_attribute2", ""),
        "Unknown attribute name",
    );
    check_throw_what(
        cs.set_interchange_attribute("unknown_attribute3", b""),
        "Unknown attribute name",
    );

    // Make sure none of the above was stored.
    let attr_map = cs.interchange_attributes();
    assert_eq!(attr_map.len(), 0);
}

/// Whether `haystack` holds `needle` (upstream's `std::string::find(...) != npos`).
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Port of `OCIO_ADD_TEST(ColorSpace, interop_id_serialization)` @ v2.5.2.
#[test]
fn interop_id_serialization() {
    let _env = crate::test_env::EnvGuard::new();
    // Test YAML serialization and deserialization of InteropID.
    let mut cfg = crate::Config::new().unwrap();
    let mut cs = ColorSpace::new();
    cs.set_name("test_colorspace");

    let interop_id = "lin_rec709_scene";

    cs.set_interop_id(interop_id).unwrap();
    cfg.add_color_space(&cs).unwrap();

    // Serialize the Config.
    let yaml_str = cfg.serialize().unwrap();

    // Verify interop_id appears in YAML.
    assert!(contains_bytes(&yaml_str, b"interop_id"));
    assert!(contains_bytes(&yaml_str, interop_id.as_bytes()));

    // Deserialize and verify.
    let deserialized_cfg = crate::Config::create_from_stream(&yaml_str).unwrap();

    // Verify interop_id is preserved.
    let deserialized_cs = deserialized_cfg.color_space("test_colorspace").unwrap();
    assert_eq!(deserialized_cs.interop_id(), interop_id.as_bytes());

    // verify that that versions earlier than 2.0 reject interop_id.
    let mut cfg_copy = cfg.clone();
    cfg_copy.set_version(2, 0).unwrap();
    cfg_copy.serialize().unwrap();

    cfg_copy.set_version(1, 0).unwrap();
    check_throw_what(
        cfg_copy.serialize(),
        "Config failed validation. The color space 'test_colorspace' has non-empty InteropID \
         and config version is less than 2.0.",
    );

    // Test with empty interop_id (should not appear in YAML).
    // (Upstream's null pointer.)
    cs.set_interop_id("").unwrap();
    cfg.add_color_space(&cs).unwrap(); // Replace the existing CS.
    let yaml_str2 = cfg.serialize().unwrap();

    // Verify empty interop_id does not appear in YAML.
    assert!(!contains_bytes(&yaml_str2, b"interop_id"));
}

/// Port of `OCIO_ADD_TEST(ColorSpace, icc_profile_name_serialization)` @ v2.5.2.
#[test]
fn icc_profile_name_serialization() {
    let _env = crate::test_env::EnvGuard::new();
    // Test YAML serialization and deserialization of IccProfileName.
    let mut cfg = crate::Config::new().unwrap();
    let mut cs = ColorSpace::new();
    cs.set_name("test_colorspace");

    let profile_name = "sRGB IEC61966-2.1";

    cs.set_interchange_attribute("icc_profile_name", profile_name)
        .unwrap();
    cfg.add_color_space(&cs).unwrap();

    // Serialize the Config.
    let yaml_str = cfg.serialize().unwrap();

    // Verify IccProfileName appears in YAML.
    assert!(contains_bytes(&yaml_str, b"icc_profile_name"));
    assert!(contains_bytes(&yaml_str, profile_name.as_bytes()));

    // Deserialize and verify.
    let deserialized_cfg = crate::Config::create_from_stream(&yaml_str).unwrap();

    // Verify IccProfileName is preserved.
    let deserialized_cs = deserialized_cfg.color_space("test_colorspace").unwrap();
    assert_eq!(
        deserialized_cs
            .interchange_attribute("icc_profile_name")
            .unwrap(),
        profile_name.as_bytes()
    );

    // verify that that earlier versions reject icc_profile_name.
    let mut cfg_copy = cfg.clone();
    cfg_copy.set_version(2, 4).unwrap();
    check_throw_what(
        cfg_copy.serialize(),
        "has non-empty interchange attributes and config version is less than 2.5.",
    );

    // Test with empty IccProfileName (should not appear in YAML, and so won't invalidate a 2.4
    // config). (Upstream's null pointer.)
    cs.set_interchange_attribute("icc_profile_name", "")
        .unwrap();
    cfg.add_color_space(&cs).unwrap(); // replace the existing CS
    let yaml_str2 = cfg.serialize().unwrap();

    // Verify empty IccProfileName does not appear in YAML.
    assert!(!contains_bytes(&yaml_str2, b"icc_profile_name"));
}

/// Port of `OCIO_ADD_TEST(Config, use_alias)` @ v2.5.2.
#[test]
fn use_alias() {
    let _env = crate::test_env::EnvGuard::new();
    const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  testAlias: aces
  default: raw

file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: default}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: aces}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    aliases: [ colorspaceAlias ]
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: colorspace
    aliases: [ aces, aces2065-1, ACES - ACES2065-1, "ACES AP0, scene-linear" ]
    family: family
    equalitygroup: group
    bitdepth: 16f
    description: |
      A raw color space.
      Second line.
    isdata: false
    categories: [one, two]
    encoding: scene-linear
    allocation: lg2
    allocationvars: [0.1, 0.9, 0.15]
    to_reference: !<LogTransform> {}
    from_reference: !<LogTransform> {}
"#;

    // Load config.

    let config = crate::Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    // Get a color space from alias.

    let cs = config.color_space("aces2065-1").unwrap();
    assert_eq!(cs.name(), b"colorspace");

    let cs = config.color_space("ACES - ACES2065-1").unwrap();
    assert_eq!(cs.name(), b"colorspace");

    assert!(config.color_space("alias no valid").is_none());

    // Get the canonical name.

    assert_eq!(config.canonical_name("aces"), b"colorspace");
    assert_eq!(
        config.canonical_name("ACES AP0, scene-linear"),
        b"colorspace"
    );
    assert_eq!(config.canonical_name("colorspace"), b"colorspace");
    assert_eq!(config.canonical_name("default"), b"raw");
    assert_eq!(config.canonical_name("DEFault"), b"raw");
    assert_eq!(config.canonical_name("not an alias"), b"");
    assert_eq!(config.canonical_name(""), b"");

    // Get the index.

    assert_eq!(config.index_for_color_space("AceS"), 1); // Case insensitve
    assert_eq!(config.index_for_color_space("aces2065-1"), 1);
    assert_eq!(config.index_for_color_space("not an alias"), -1);

    // Get color space referenced by alias in role.

    let cs = config.color_space("testAlias").unwrap();
    assert_eq!(cs.name(), b"colorspace");

    // Color space from string.

    assert_eq!(
        config.color_space_from_filepath("test_aces_test").unwrap(),
        b"colorspace"
    );
    // "colorspace" is present but "ColorspaceAlias" is longer (and at the same position).
    assert_eq!(
        config
            .color_space_from_filepath("skdj_ColorspaceAlias_dfjdk")
            .unwrap(),
        b"raw"
    );

    // With inactive color spaces.

    let mut cfg = (*config).clone();
    cfg.set_inactive_color_spaces("colorspace");

    assert_eq!(
        cfg.color_space_from_filepath("test_aces_test").unwrap(),
        b"colorspace"
    );
}

/// Port of `OCIO_ADD_TEST(Config, color_space_serialize)` @ v2.5.2.
#[test]
fn color_space_serialize() {
    let _env = crate::test_env::EnvGuard::new();
    const START: &str = r#"ocio_profile_version: 2

environment:
  {}
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

active_displays: []
active_views: []

"#;
    let load = |cfg_string: &str| crate::Config::create_from_stream(cfg_string.as_bytes()).unwrap();
    let cs_at = |config: &crate::Config, i: i32| {
        config
            .color_space(config.color_space_name_by_index(i))
            .unwrap()
            .clone()
    };

    // The raw config.
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform
"#;
        let cfg_string = [START, END].concat();

        // Load config.

        let config = load(&cfg_string);
        config.validate().unwrap();

        // Check colorspace.

        assert_eq!(config.num_color_spaces(), 1);
        let cs = cs_at(&config, 0);
        assert_eq!(cs.allocation(), Allocation::Uniform);
        assert_eq!(cs.allocation_num_vars(), 0);
        assert_eq!(cs.bit_depth(), BitDepth::F32);
        assert_eq!(
            cs.description(),
            b"A raw color space. Conversions to and from this space are no-ops."
        );
        assert_eq!(cs.encoding(), b"");
        assert_eq!(cs.equality_group(), b"");
        assert_eq!(cs.family(), b"raw");
        assert_eq!(cs.name(), b"raw");
        assert_eq!(cs.num_categories(), 0);
        assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);
        assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
        assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
        assert!(cs.is_data());

        // Save and compare output with input.

        assert_eq!(cfg_string.as_bytes(), config.serialize().unwrap());
    }

    // Adding a color space that uses all parameters (as of 2.0).
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: colorspace
    aliases: [alias1, alias2]
    family: family
    equalitygroup: group
    bitdepth: 16f
    description: |
      A raw color space.
      Second line.
    isdata: false
    categories: [one, two]
    encoding: scene-linear
    allocation: lg2
    allocationvars: [0.1, 0.9, 0.15]
    to_scene_reference: !<LogTransform> {}
    from_scene_reference: !<LogTransform> {}
"#;
        let cfg_string = [START, END].concat();

        // Load config.

        let config = load(&cfg_string);
        config.validate().unwrap();

        // Check colorspace.

        assert_eq!(config.num_color_spaces(), 2);
        let cs = cs_at(&config, 1);
        assert_eq!(cs.allocation(), Allocation::Lg2);
        assert_eq!(cs.allocation_num_vars(), 3);
        let vars = cs.allocation_vars();
        assert_eq!(vars[0], 0.1f32);
        assert_eq!(vars[1], 0.9f32);
        assert_eq!(vars[2], 0.15f32);
        assert_eq!(cs.bit_depth(), BitDepth::F16);
        assert_eq!(cs.description(), b"A raw color space.\nSecond line.");
        assert_eq!(cs.encoding(), b"scene-linear");
        assert_eq!(cs.equality_group(), b"group");
        assert_eq!(cs.family(), b"family");
        assert_eq!(cs.name(), b"colorspace");
        assert_eq!(cs.num_aliases(), 2);
        assert_eq!(cs.alias(0), b"alias1");
        assert_eq!(cs.alias(1), b"alias2");
        assert_eq!(cs.num_categories(), 2);
        assert_eq!(cs.category(0).unwrap(), b"one");
        assert_eq!(cs.category(1).unwrap(), b"two");
        assert_eq!(cs.reference_space_type(), ReferenceSpaceType::Scene);
        assert!(cs.transform(ColorSpaceDirection::ToReference).is_some());
        assert!(cs.transform(ColorSpaceDirection::FromReference).is_some());
        assert!(!cs.is_data());

        // Save and compare output with input.

        assert_eq!(cfg_string.as_bytes(), config.serialize().unwrap());
    }

    // Description trailing newlines are removed.
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: raw2
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: |
      One line.

      Other line.
    isdata: true
    allocation: uniform
"#;
        let cfg_string = [START, END].concat();

        // Load config.

        let config = load(&cfg_string);
        config.validate().unwrap();

        // Check colorspace.

        assert_eq!(config.num_color_spaces(), 2);
        let cs = cs_at(&config, 0);
        // Description has no trailing \n.
        assert_eq!(cs.description(), b"Some text.");

        let cs = cs_at(&config, 1);
        // Description has no trailing \n.
        assert_eq!(cs.description(), b"One line.\n\nOther line.");

        // Save and compare output with input.

        assert_eq!(cfg_string.as_bytes(), config.serialize().unwrap());

        // Even if some line feeds are added to the end of description they won't be saved.
        let mut cs_edit = cs.clone();

        cs_edit.set_description("One line.\n\nOther line.\n");

        let mut config_edit = (*config).clone();
        config_edit.add_color_space(&cs_edit).unwrap();

        assert_eq!(cfg_string.as_bytes(), config_edit.serialize().unwrap());

        // Even if several line feeds are added.

        cs_edit.set_description("One line.\n\nOther line.\n\n\n\n");
        config_edit.add_color_space(&cs_edit).unwrap();

        assert_eq!(cfg_string.as_bytes(), config_edit.serialize().unwrap());

        // Single line descriptions are saved on one line and trailing \n are ignored.

        let cs = cs_at(&config, 0);
        assert_eq!(cs.description(), b"Some text.");

        let mut cs_edit = cs.clone();
        cs_edit.set_description("Some text.\n\n\n");
        config_edit.add_color_space(&cs_edit).unwrap();

        assert_eq!(cfg_string.as_bytes(), config_edit.serialize().unwrap());
    }

    // Test different way of writing description, some are not written as they would be saved.
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    description: |
      "Some text."

  - !<ColorSpace>
    name: raw2
    description: "Multiple lines\n\nOther line.\n\n\n"

  - !<ColorSpace>
    name: raw3
    description: |
      Test \n backslash+n.

  - !<ColorSpace>
    name: raw4
    description: "One"

  - !<ColorSpace>
    name: raw5
    description: More "than" one

  - !<ColorSpace>
    name: raw6
    description: Other \n test.

  - !<ColorSpace>
    name: raw7
    description: Double backslash+n \\n test.

  - !<ColorSpace>
    name: raw8
    description: "Double backslash+n \\n in quotes."
"#;
        let cfg_string = [START, END].concat();

        // Load config.

        let config = load(&cfg_string);
        config.validate().unwrap();

        // Check colorspace descriptions.

        assert_eq!(config.num_color_spaces(), 8);
        // description: |
        //   "Some text."
        // A single line comment can be written using the multi-line syntax. Note that
        // surounding quotes are preserved when multi-line syntax is used.
        assert_eq!(cs_at(&config, 0).description(), b"\"Some text.\"");

        // description: "Multiple lines\n\nOther line.\n\n\n"
        // Multi-lines comment can be written using the single line syntax when "" are used.
        // Note that trailing newlines are removed.
        assert_eq!(
            cs_at(&config, 1).description(),
            b"Multiple lines\n\nOther line."
        );

        // description: |
        //     Test \n backslash+n.
        // Without "" \n is just a backslash '\' on a 'n'. Would be written using single line
        // syntax.
        assert_eq!(cs_at(&config, 2).description(), b"Test \\n backslash+n.");

        // description: "One"
        // Surrounding "" for single line comment are removed.
        assert_eq!(cs_at(&config, 3).description(), b"One");

        // description: More "than" one
        // In between "" are preserved.
        assert_eq!(cs_at(&config, 4).description(), b"More \"than\" one");

        // description: Other \n test.
        assert_eq!(cs_at(&config, 5).description(), b"Other \\n test.");

        // description: Double backslash+n \\n test.
        assert_eq!(
            cs_at(&config, 6).description(),
            b"Double backslash+n \\\\n test."
        );

        // description: "Double backslash+n \\n in quotes."
        assert_eq!(
            cs_at(&config, 7).description(),
            b"Double backslash+n \\n in quotes."
        );

        const END_RES: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: "\"Some text.\""
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: |
      Multiple lines

      Other line.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw3
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Test \n backslash+n.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw4
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: One
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw5
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: More "than" one
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw6
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Other \n test.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw7
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Double backslash+n \\n test.
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: raw8
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    description: Double backslash+n \n in quotes.
    isdata: false
    allocation: uniform
"#;
        let cfg_res = [START, END_RES].concat();

        assert_eq!(cfg_res.as_bytes(), config.serialize().unwrap());
    }

    // Test that the interop_id is valid in v2.0 config too.
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    aliases: [ data ]
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
        let config = load(&[START, END].concat());
        let cs = config.color_space("raw").unwrap();
        assert_eq!(cs.interop_id(), b"data");

        config.validate().unwrap();
    }

    // Test that the undefined interop_id does not pass validation
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
        let config = load(&[START, END].concat());
        let cs = config.color_space("raw").unwrap();
        assert_eq!(cs.interop_id(), b"data");

        check_throw_what(
            config.validate(),
            "Config failed color space validation. The color space 'raw' refers to an interop \
             ID, 'data', which is not a color space name or alias.",
        );
    }

    // Test that the interop id can be found in another color space.
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: one data color space.
    isdata: true
    allocation: uniform
  - !<ColorSpace>
    name: data
    interop_id: data
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: another data color space.
    isdata: true
    allocation: uniform
"#;
        let config = load(&[START, END].concat());

        config.validate().unwrap();
    }

    // Test that the interchange is NOT valid in v2.0 config.
    {
        const END: &str = r#"colorspaces:
  - !<ColorSpace>
    name: raw
    interchange:
        amf_transform_ids: should NOT be valid in 2.0 config
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;
        check_throw_what(
            crate::Config::create_from_stream([START, END].concat().as_bytes()),
            "Config failed validation. The color space 'raw' has non-empty interchange \
             attributes and config version is less than 2.5.",
        );
    }

    // Interchange tests in 2.5
    const START_2_5: &str = r#"ocio_profile_version: 2.5

environment:
  {}
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

active_displays: []
active_views: []

"#;

    // Test that the interchange is valid in v2.5 config.
    {
        const END_AMF: &str = r#"
colorspaces:
  - !<ColorSpace>
    name: raw
    interchange:
        amf_transform_ids: This is valid in 2.5 config
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;

        let config = load(&[START_2_5, END_AMF].concat());
        let attr_map = config.color_space("raw").unwrap().interchange_attributes();
        assert_eq!(attr_map.len(), 1);
    }

    // Test that the unknown interchange attrib will be ignored in 2.5.
    {
        const END_UNKOWN: &str = r#"
colorspaces:
  - !<ColorSpace>
    name: raw
    interchange:
        my-attrib: will be ignored
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: Some text.
    isdata: true
    allocation: uniform
"#;

        let cfg_string = [START_2_5, END_UNKOWN].concat();
        let (config, log) = crate::test_env::capture_log(|| {
            crate::Config::create_from_stream(cfg_string.as_bytes())
        });
        let config = config.unwrap();
        assert_eq!(
            log.concat(),
            b"[OpenColorIO Warning]: Unknown key in interchange: 'my-attrib'.\n"
        );
        let attr_map = config.color_space("raw").unwrap().interchange_attributes();
        assert_eq!(attr_map.len(), 0);
    }
}
