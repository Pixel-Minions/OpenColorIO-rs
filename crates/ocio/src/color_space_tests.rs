// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the color space: `tests/cpu/ColorSpace_tests.cpp` @ v2.5.2. The tests that load or
//! serialize configs come with the YAML reader and writer (WP 3.3j, 3.7b), the processor ones
//! with Phase 2 and ConfigUtils (Phase 9). The text is compared with the wheel's in
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
