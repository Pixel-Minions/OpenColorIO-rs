// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the no-ops. Upstream's `NoOps` tests (tests/cpu/ops/noop/NoOps_tests.cpp @ v2.5.2)
//! compare each no-op with a Matrix op; they come with the Matrix op.

use std::sync::Arc;

use super::*;
use crate::op_data::OpDataType;
use crate::open_color_types::DynamicPropertyType;

const ALL_DYNAMIC_TYPES: [DynamicPropertyType; 7] = [
    DynamicPropertyType::Exposure,
    DynamicPropertyType::Contrast,
    DynamicPropertyType::Gamma,
    DynamicPropertyType::GradingPrimary,
    DynamicPropertyType::GradingRgbCurve,
    DynamicPropertyType::GradingTone,
    DynamicPropertyType::GradingHueCurve,
];

fn file_and_look() -> OpVec {
    let mut ops = OpVec::new();
    create_file_no_op(&mut ops, b"dir/file.clf");
    create_look_no_op(&mut ops, b"-look");
    ops
}

#[test]
fn create_appends_one_op_each() {
    let ops = file_and_look();
    assert_eq!(ops.len(), 2);
    // The FileNoOp info appears in upstream's file format tests, e.g.
    // tests/cpu/fileformats/FileFormatIridasCube_tests.cpp:267 @ v2.5.2.
    assert_eq!(ops[0].get_info(), "<FileNoOp>");
    assert_ne!(ops[1].get_info(), ops[0].get_info());

    let OpData::NoOp(file) = &**ops[0].data();
    assert_eq!(file.file_data().unwrap().get_path(), b"dir/file.clf");
    assert!(!file.file_data().unwrap().get_complete());
    let OpData::NoOp(look) = &**ops[1].data();
    assert!(look.file_data().is_none());
    assert!(matches!(look.kind(), NoOpKind::Look(name) if name == b"-look"));
}

#[test]
fn the_no_ops_leave_pixels_alone() {
    // What upstream's NoOps tests check of each no-op (NoOps_tests.cpp:284-345 @ v2.5.2).
    for op in file_and_look().iter() {
        assert!(op.is_no_op_type());
        assert_eq!(op.data().get_type(), OpDataType::NoOp);
        assert!(op.is_no_op());
        assert!(op.is_identity());
        assert!(!op.has_channel_crosstalk());
        assert!(op.supported_by_legacy_shader());
        assert!(op.validate().is_ok());
        assert!(op.get_cpu_op(false).unwrap().is_none());
        assert!(op.get_cpu_op(true).unwrap().is_none());

        let pixels = [0.1f32, -2.0, f32::NAN, 1.0e30, 5.0, 6.0, 7.0, 8.0];
        let mut in_place = pixels;
        op.apply(&mut in_place).unwrap();
        assert_eq!(in_place.map(f32::to_bits), pixels.map(f32::to_bits));

        let mut out = [0.0f32; 8];
        op.apply_in_out(&pixels, &mut out).unwrap();
        assert_eq!(out.map(f32::to_bits), pixels.map(f32::to_bits));
    }
}

#[test]
fn same_type_and_inverse_mean_the_same_class() {
    let ops = file_and_look();
    let mut more = OpVec::new();
    create_file_no_op(&mut more, b"other.clf");
    create_look_no_op(&mut more, b"other");

    for (i, op) in ops.iter().enumerate() {
        for (j, other) in more.iter().enumerate() {
            assert_eq!(op.is_same_type(other), i == j, "{op} and {other}");
            assert_eq!(op.is_inverse(other), i == j, "{op} and {other}");
        }
        assert!(op.is_same_type(op));
        assert!(op.is_inverse(op));
    }
}

#[test]
fn the_no_ops_do_not_combine() {
    let ops = file_and_look();
    for op in ops.iter() {
        for other in ops.iter() {
            assert!(!op.can_combine_with(other).unwrap());
            let mut result = OpVec::new();
            let err = op.combine_with(&mut result, other).unwrap_err();
            assert!(err.to_string().contains(op.get_info()), "{err}");
            assert!(result.is_empty());
        }
    }
}

#[test]
fn the_no_ops_have_no_dynamic_property() {
    for mut op in file_and_look().to_vec() {
        assert!(!op.is_dynamic());
        for type_ in ALL_DYNAMIC_TYPES {
            assert!(!op.has_dynamic_property(type_));
            // tests/cpu/Op_tests.cpp:271-272 @ v2.5.2.
            ocio_testkit::upstream::check_throw_what(
                op.get_dynamic_property(type_),
                "does not implement dynamic property",
            );
        }

        let prop: crate::dynamic_property::DynamicPropertyRcPtr =
            Arc::new(crate::dynamic_property::DynamicPropertyDoubleImpl::new(
                DynamicPropertyType::Exposure,
                1.0,
                true,
            ))
            .into();
        assert!(
            op.replace_dynamic_property(DynamicPropertyType::Exposure, &prop)
                .is_err()
        );

        let data = Arc::clone(op.data());
        op.remove_dynamic_properties();
        assert!(Arc::ptr_eq(&data, op.data()));
    }
}

#[test]
fn finalize_keeps_the_data() {
    for mut op in file_and_look().to_vec() {
        let data = Arc::clone(op.data());
        op.finalize().unwrap();
        assert!(Arc::ptr_eq(&data, op.data()));
    }
}

#[test]
fn the_cache_id_of_a_look_is_its_name_and_a_file_has_none() {
    let ops = file_and_look();
    // FileNoOp::getCacheID returns m_fileReference, which is never set (I-40).
    assert_eq!(ops[0].get_cache_id(), b"");
    assert_eq!(ops[1].get_cache_id(), b"-look");

    // Bytes pass through, NUL included: upstream's std::string keeps them.
    let mut odd = OpVec::new();
    create_look_no_op(&mut odd, b"a\0\xff");
    assert_eq!(odd[0].get_cache_id(), b"a\0\xff");
}

#[test]
fn clone_op_makes_new_data_of_the_same_class() {
    let named = |kind| {
        let mut data = OpData::NoOp(NoOpData::new(kind));
        data.set_name(b"named");
        Op::new(data)
    };
    let ops = [
        named(NoOpKind::File(FileNoOpData::new(b"dir/file.clf"))),
        named(NoOpKind::Look(b"-look".to_vec())),
    ];
    let OpData::NoOp(file) = &**ops[0].data();
    file.file_data().unwrap().set_complete();
    assert!(file.file_data().unwrap().get_complete());

    for op in ops.iter() {
        assert_eq!(op.data().get_name(), b"named");
        let clone = op.clone_op();
        assert!(!Arc::ptr_eq(op.data(), clone.data()));
        assert!(op.is_same_type(&clone));
        assert_eq!(clone.get_info(), op.get_info());
        assert_eq!(clone.get_cache_id(), op.get_cache_id());
        // New data: empty metadata.
        assert_eq!(clone.data().get_name(), b"");
    }

    // A new FileNoOpData is still being loaded, and has the path.
    let clone = ops[0].clone_op();
    let OpData::NoOp(cloned) = &**clone.data();
    assert_eq!(cloned.file_data().unwrap().get_path(), b"dir/file.clf");
    assert!(!cloned.file_data().unwrap().get_complete());
}

#[test]
fn set_complete_reaches_every_op_sharing_the_data() {
    // FileTransform keeps the op it appends and marks its data complete once the file is
    // loaded (src/OpenColorIO/transforms/FileTransform.cpp:944-961 @ v2.5.2).
    let ops = file_and_look();
    let kept = ops[0].clone();
    let OpData::NoOp(data) = &**kept.data();
    data.file_data().unwrap().set_complete();
    let OpData::NoOp(in_list) = &**ops[0].data();
    assert!(in_list.file_data().unwrap().get_complete());
}
