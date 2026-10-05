// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the named transform. Upstream's `basic` and `alias`
//! (`tests/cpu/NamedTransform_tests.cpp` @ v2.5.2) add named transforms to a config and call
//! `NamedTransform::GetTransform`, so they come with the config's named transforms (WP 3.4j)
//! and the color space transform's builder (WP 3.2a); these tests check the parts that need
//! neither, with upstream's values. The text is compared with the wheel's in
//! `tests/model_objects_oracle.rs`.

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
    named_transform.set_transform(Some(&mat), TransformDirection::Forward);
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
