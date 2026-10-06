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

    // The message is the wheel's, checked in tests/transform_oracle.rs.
    for index in [-1, 2] {
        let message = group.transform(index).unwrap_err().message().to_string();
        assert_eq!(group.transform_mut(index).unwrap_err().message(), message);
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
    let config = Config::create_raw().unwrap();
    let mut group = GroupTransform::new();
    group
        .format_metadata_mut()
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();
    let mut ops = OpVec::new();
    crate::transform::build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &group.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert!(ops.is_empty());
    assert_eq!(ops.get_format_metadata().get_num_attributes(), 1);
}

/// A group with the given metadata attribute and children.
fn group_with(name: &[u8], dir: TransformDirection, children: Vec<Transform>) -> GroupTransform {
    let mut group = GroupTransform::new();
    group.set_direction(dir);
    group
        .format_metadata_mut()
        .add_attribute(Some(b"name"), Some(name))
        .unwrap();
    for child in children {
        group.append_transform(child);
    }
    group
}

/// Nested groups: each group copies its metadata while no op is built yet, so the innermost
/// group met first before any op wins over the groups around it; a group met after an op
/// copies nothing. Inverse, the children come in reverse order, so the last group is met
/// first.
#[test]
fn build_ops_copies_the_metadata_of_the_groups_met_before_any_op() {
    use crate::transforms::matrix_transform::MatrixTransform;

    let config = Config::create_raw().unwrap();
    let build = |group: &GroupTransform, dir: TransformDirection| {
        let mut ops = OpVec::new();
        crate::transform::build_ops(
            &mut ops,
            &config,
            &config.current_context().get(),
            &group.clone().into(),
            dir,
        )
        .unwrap();
        assert_eq!(ops.len(), 2);
        ops.get_format_metadata().clone()
    };
    let matrix = || Transform::from(MatrixTransform::new());
    let first = group_with(
        b"first",
        TransformDirection::Forward,
        vec![group_with(b"innermost", TransformDirection::Forward, vec![matrix()]).into()],
    );
    let last = group_with(b"last", TransformDirection::Forward, vec![matrix()]);
    let outer = group_with(
        b"outer",
        TransformDirection::Forward,
        vec![first.clone().into(), last.clone().into()],
    );

    let innermost = match first.transform(0).unwrap() {
        Transform::Group(g) => g.format_metadata().clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        build(&outer, TransformDirection::Forward).get_attribute_value(0),
        innermost.get_attribute_value(0)
    );
    assert_eq!(
        build(&outer, TransformDirection::Inverse).get_attribute_value(0),
        last.format_metadata().get_attribute_value(0)
    );

    // An op before the groups: none of them copies, the outer group's own included.
    let mut ops = OpVec::new();
    for transform in [matrix(), outer.into()] {
        crate::transform::build_ops(
            &mut ops,
            &config,
            &config.current_context().get(),
            &transform,
            TransformDirection::Forward,
        )
        .unwrap();
    }
    assert_eq!(ops.len(), 3);
    assert_eq!(ops.get_format_metadata().get_num_attributes(), 0);
}
