// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the group transform. Upstream's (`tests/cpu/transforms/GroupTransform_tests.cpp`)
//! need the Matrix and FixedFunction transforms (`basic`) or the file writers
//! (`write_formats`, `write_with_noops`); they come with them. The text and the validation are
//! compared with the wheel's in `tests/transform_oracle.rs`.

use super::*;

/// A new group is forward, empty, with the root metadata; children keep their order, and an
/// index outside them is upstream's error.
#[test]
fn children_and_indices() {
    let mut group = GroupTransform::new();
    assert_eq!(group.direction(), TransformDirection::Forward);
    assert_eq!(group.num_transforms(), 0);
    assert_eq!(
        group.format_metadata().get_element_name(),
        ocio_ops::format_metadata::METADATA_ROOT
    );

    let mut inner = GroupTransform::new();
    inner.set_direction(TransformDirection::Inverse);
    group.append_transform(inner.into());
    group.prepend_transform(GroupTransform::new().into());
    assert_eq!(group.num_transforms(), 2);
    assert_eq!(
        group.transform(0).unwrap().direction(),
        TransformDirection::Forward
    );
    assert_eq!(
        group.transform(1).unwrap().direction(),
        TransformDirection::Inverse
    );
    group
        .transform_mut(0)
        .unwrap()
        .set_direction(TransformDirection::Inverse);
    assert_eq!(
        group.transform(0).unwrap().direction(),
        TransformDirection::Inverse
    );

    for index in [-1, 2] {
        assert_eq!(
            group.transform(index).unwrap_err().message(),
            format!("Invalid transform index {index}.")
        );
        assert!(group.transform_mut(index).is_err());
    }

    // A copy owns its children (the owner's decision; upstream shares them, I-11).
    let copy = group.clone();
    group
        .transform_mut(1)
        .unwrap()
        .set_direction(TransformDirection::Forward);
    assert_eq!(
        copy.transform(1).unwrap().direction(),
        TransformDirection::Inverse
    );
    group.validate().unwrap();
}

/// The first group's metadata becomes the ops' metadata, and an empty group builds no ops.
#[test]
fn build_ops_copies_the_first_group_metadata() {
    let config = Config::create_raw();
    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();
    let mut ops = OpVec::new();
    crate::transform::build_ops(
        &mut ops,
        &config,
        config.current_context(),
        &group.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert!(ops.is_empty());
    assert_eq!(ops.get_format_metadata().get_num_attributes(), 1);
}
