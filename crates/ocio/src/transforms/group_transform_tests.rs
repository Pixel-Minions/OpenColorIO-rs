// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the group transform: `tests/cpu/transforms/GroupTransform_tests.cpp` @ v2.5.2.
//! `basic` came with the FixedFunction transform (`p3-after-p2`); `write_formats` and
//! `write_with_noops` with the CLF/CTF writer (`p4-ctf-read-3`, WP 4.6e). The text and the
//! validation are compared with the wheel's in `tests/transform_oracle.rs`.

use super::*;

/// Port of `OCIO_ADD_TEST(GroupTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    use crate::transforms::fixed_function_transform::FixedFunctionTransform;
    use crate::transforms::matrix_transform::MatrixTransform;
    use ocio_ops::format_metadata::METADATA_ROOT;
    use ocio_ops::open_color_types::FixedFunctionStyle;

    let mut group = GroupTransform::new();
    assert_eq!(group.direction(), TransformDirection::Forward);

    group.set_direction(TransformDirection::Inverse);
    assert_eq!(group.direction(), TransformDirection::Inverse);

    assert_eq!(group.num_transforms(), 0);

    let group_data = group.format_metadata();
    assert_eq!(group_data.get_element_name(), METADATA_ROOT);
    assert_eq!(group_data.get_num_attributes(), 0);
    assert_eq!(group_data.get_num_children_elements(), 0);

    let matrix = MatrixTransform::new();
    group.append_transform(matrix.into());
    let ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[]).unwrap();
    group.append_transform(ff.into());

    assert_eq!(group.num_transforms(), 2);

    let t0 = group.transform(0).unwrap();
    assert!(matches!(t0, Transform::Matrix(_)));

    let t1 = group.transform(1).unwrap();
    assert!(matches!(t1, Transform::FixedFunction(_)));

    let metadata = group.format_metadata_mut();
    assert_eq!(metadata.get_element_name(), METADATA_ROOT);
    assert_eq!(metadata.get_element_value(), b"");
    assert_eq!(metadata.get_num_attributes(), 0);
    assert_eq!(metadata.get_num_children_elements(), 0);
    metadata
        .add_attribute(Some(b"att1"), Some(b"val1"))
        .unwrap();
    metadata
        .add_child_element(Some(b"child1"), Some(b"content1"))
        .unwrap();
}

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

/// A writer whose writes all fail, as a C++ stream in a failed state.
struct FailingWriter;

impl std::io::Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("the stream failed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("the stream failed"))
    }
}

/// A failure of the output stream goes unreported, as upstream's `std::ostream` keeps it in
/// its state bits (I-178); the format's own errors are still raised.
#[test]
fn a_failing_output_stream_is_not_an_error() {
    let config = Config::new().expect("a config");
    let mut group = GroupTransform::new();
    group.append_transform(crate::transforms::cdl_transform::CdlTransform::new().into());
    group
        .write(&config, "ColorCorrection", &mut FailingWriter)
        .expect("the stream's failure is not reported");

    let empty = GroupTransform::new();
    let err = empty
        .write(&config, "ColorCorrection", &mut FailingWriter)
        .expect_err("the format's error");
    assert!(err.to_string().contains("there should be a single CDL"));
}

/// The name of the write format of the extension `extension` (ignoring case), or `""`.
///
/// Port of `GetFormatName` (tests/cpu/transforms/GroupTransform_tests.cpp:55-68 @ v2.5.2).
fn get_format_name(extension: &str) -> Vec<u8> {
    // All extensions are lower case.
    let requested_ext = extension.to_ascii_lowercase();
    let num_formats = GroupTransform::num_write_formats();
    for i in 0..num_formats {
        let format_ext = GroupTransform::format_extension_by_index(i);
        if requested_ext.as_bytes() == format_ext {
            return GroupTransform::format_name_by_index(i).to_vec();
        }
    }
    Vec::new()
}

/// Port of `OCIO_ADD_TEST(GroupTransform, write_formats)` @ v2.5.2.
#[test]
fn write_formats() {
    use crate::transforms::file_format::{
        FILEFORMAT_CLF, FILEFORMAT_COLOR_CORRECTION, FILEFORMAT_COLOR_CORRECTION_COLLECTION,
        FILEFORMAT_COLOR_DECISION_LIST, FILEFORMAT_CTF,
    };

    assert_eq!(GroupTransform::num_write_formats(), 5);

    assert_eq!(get_format_name("CLF"), FILEFORMAT_CLF.as_bytes());
    assert_eq!(get_format_name("CTF"), FILEFORMAT_CTF.as_bytes());
    assert_eq!(
        get_format_name("cc"),
        FILEFORMAT_COLOR_CORRECTION.as_bytes()
    );
    assert_eq!(
        get_format_name("ccc"),
        FILEFORMAT_COLOR_CORRECTION_COLLECTION.as_bytes()
    );
    assert_eq!(
        get_format_name("cdl"),
        FILEFORMAT_COLOR_DECISION_LIST.as_bytes()
    );
    assert!(get_format_name("XXX").is_empty());
}

/// Port of `OCIO_ADD_TEST(GroupTransform, write_with_noops)` @ v2.5.2.
#[test]
fn write_with_noops() {
    use crate::fileformats::test_utils::get_test_files_dir;
    use crate::transforms::file_format::FILEFORMAT_CLF;
    use crate::transforms::file_transform::FileTransform;

    // The unit test validates that the no-ops are transparent when writing to a lut file
    // format.

    let config = Config::new().unwrap();

    // `CreateFileTransform` (tests/cpu/UnitTestUtils.cpp:39-48 @ v2.5.2).
    let create_file_transform = |file_name: &str| {
        let mut file = FileTransform::new();
        file.set_src(format!("{}/{file_name}", get_test_files_dir()));
        file
    };

    // Step 1 - Write to CLF from a group transform.

    {
        let file = create_file_transform("logtolin_8to8.lut");

        let mut group = GroupTransform::new();
        group.append_transform(Transform::File(file));

        let mut oss = Vec::new();
        group.write(&config, FILEFORMAT_CLF, &mut oss).unwrap();
    }

    // Step 2 - Write to CLF from a processor.

    {
        let file = create_file_transform("logtolin_8to8.lut");

        let proc = config.processor(&Transform::File(file)).unwrap();
        let group = proc.create_group_transform().unwrap();

        let mut oss = Vec::new();
        group.write(&config, FILEFORMAT_CLF, &mut oss).unwrap();
    }
}
