// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the no-ops, with upstream's `allocation_op`, `file_op` and `look_op` tests
//! (tests/cpu/ops/noop/NoOps_tests.cpp @ v2.5.2). Its `throw` and `partition_gpu_ops` tests
//! test `PartitionGPUOps`, which needs the Lut3D op.
//!
//! The rest checks how the ops share and copy their data, and relations upstream's code
//! states (a look's cache ID is its name, an allocation's its data's). What the wheel shows of
//! the file and look no-ops, their `getInfo` and cache IDs in the debug log and their removal by
//! the optimizer, is checked against it in `tests/cpu_processor_oracle.rs`
//! (`the_no_op_types_match_the_wheel`).

use std::sync::Arc;

use super::*;
use crate::op_data::OpDataType;
use crate::open_color_types::{Allocation, DynamicPropertyType, TransformDirection};
use crate::ops::matrix::matrix_op::create_scale_op;

const ALL_DYNAMIC_TYPES: [DynamicPropertyType; 7] = [
    DynamicPropertyType::Exposure,
    DynamicPropertyType::Contrast,
    DynamicPropertyType::Gamma,
    DynamicPropertyType::GradingPrimary,
    DynamicPropertyType::GradingRgbCurve,
    DynamicPropertyType::GradingTone,
    DynamicPropertyType::GradingHueCurve,
];

/// The allocation of upstream's `CreateGenericAllocationOp` (NoOps_tests.cpp:17-24 @ v2.5.2).
fn lg2_allocation() -> AllocationData {
    AllocationData {
        allocation: Allocation::Lg2,
        vars: vec![-8.0, 8.0],
    }
}

/// An `AllocationNoOp`, a `FileNoOp` and a `LookNoOp`.
fn the_no_ops() -> OpVec {
    let mut ops = OpVec::new();
    create_gpu_allocation_no_op(&mut ops, &lg2_allocation());
    create_file_no_op(&mut ops, b"dir/file.clf");
    create_look_no_op(&mut ops, b"-look");
    ops
}

fn no_op_data(op: &Op) -> &NoOpData {
    match &**op.data() {
        OpData::NoOp(data) => data,
        OpData::Log(_)
        | OpData::FixedFunction(_)
        | OpData::Cdl(_)
        | OpData::Gamma(_)
        | OpData::Lut1D(_)
        | OpData::Matrix(_)
        | OpData::Range(_)
        | OpData::Exponent(_)
        | OpData::GradingRgbCurve(_)
        | OpData::Lut3D(_)
        | OpData::Reference(_) => {
            panic!("{op} isn't a no-op")
        }
    }
}

#[test]
fn create_appends_one_op_each() {
    let ops = the_no_ops();
    assert_eq!(ops.len(), 3);
    // The FileNoOp info appears in upstream's file format tests, e.g.
    // tests/cpu/fileformats/FileFormatIridasCube_tests.cpp:267 @ v2.5.2.
    assert_eq!(ops[1].get_info(), "<FileNoOp>");
    assert_ne!(ops[0].get_info(), ops[1].get_info());
    assert_ne!(ops[0].get_info(), ops[2].get_info());
    assert_ne!(ops[1].get_info(), ops[2].get_info());

    let allocation = no_op_data(&ops[0]);
    assert_eq!(allocation.get_gpu_allocation(), Some(&lg2_allocation()));
    assert!(allocation.file_data().is_none());

    let file = no_op_data(&ops[1]);
    assert_eq!(file.file_data().unwrap().get_path(), b"dir/file.clf");
    assert!(!file.file_data().unwrap().get_complete());
    assert!(file.get_gpu_allocation().is_none());

    let look = no_op_data(&ops[2]);
    assert!(look.file_data().is_none());
    assert!(look.get_gpu_allocation().is_none());
    assert!(matches!(look.kind(), NoOpKind::Look(name) if name == b"-look"));
}

#[test]
fn the_no_ops_leave_pixels_alone() {
    // What upstream's NoOps tests check of each no-op (NoOps_tests.cpp:284-345 @ v2.5.2).
    for op in the_no_ops().iter_mut() {
        assert!(op.is_no_op_type());
        assert_eq!(op.data().get_type(), OpDataType::NoOp);
        assert!(op.is_no_op().unwrap());
        assert!(op.is_identity().unwrap());
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
    let ops = the_no_ops();
    let mut more = OpVec::new();
    create_gpu_allocation_no_op(&mut more, &AllocationData::default());
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
    let ops = the_no_ops();
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
    for mut op in the_no_ops().to_vec() {
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
    for mut op in the_no_ops().to_vec() {
        let data = Arc::clone(op.data());
        op.finalize().unwrap();
        assert!(Arc::ptr_eq(&data, op.data()));
    }
}

#[test]
fn the_cache_ids() {
    let ops = the_no_ops();
    // An allocation's cache ID is its data's.
    assert_eq!(
        ops[0].get_cache_id().unwrap(),
        lg2_allocation().get_cache_id().into_bytes()
    );
    // FileNoOp::getCacheID returns m_fileReference, which is never set (I-40).
    assert_eq!(ops[1].get_cache_id().unwrap(), b"");
    // A look's is its name.
    assert_eq!(ops[2].get_cache_id().unwrap(), b"-look");

    // Bytes pass through, NUL included: upstream's std::string keeps them.
    let mut odd = OpVec::new();
    create_look_no_op(&mut odd, b"a\0\xff");
    assert_eq!(odd[0].get_cache_id().unwrap(), b"a\0\xff");

    // The data's cache ID is empty for each.
    for op in ops.iter() {
        assert_eq!(op.data().get_cache_id().unwrap(), b"");
    }
}

#[test]
fn clone_op_makes_new_data_of_the_same_class() {
    let named = |kind| {
        let mut data = OpData::NoOp(NoOpData::new(kind));
        data.set_name(b"named");
        Op::new(data)
    };
    let ops = [
        named(NoOpKind::Allocation(lg2_allocation())),
        named(NoOpKind::File(FileNoOpData::new(b"dir/file.clf"))),
        named(NoOpKind::Look(b"-look".to_vec())),
    ];
    let file = no_op_data(&ops[1]).file_data().unwrap();
    file.set_complete();
    assert!(file.get_complete());

    for op in ops.iter() {
        assert_eq!(op.data().get_name(), b"named");
        let clone = op.clone_op().unwrap();
        assert!(!Arc::ptr_eq(op.data(), clone.data()));
        assert!(op.is_same_type(&clone));
        assert_eq!(clone.get_info(), op.get_info());
        assert_eq!(clone.get_cache_id(), op.get_cache_id());
        // New data: empty metadata.
        assert_eq!(clone.data().get_name(), b"");
    }

    // The allocation is copied.
    let clone = ops[0].clone_op().unwrap();
    assert_eq!(
        no_op_data(&clone).get_gpu_allocation(),
        Some(&lg2_allocation())
    );

    // A new FileNoOpData is still being loaded, and has the path.
    let clone = ops[1].clone_op().unwrap();
    let cloned = no_op_data(&clone).file_data().unwrap();
    assert_eq!(cloned.get_path(), b"dir/file.clf");
    assert!(!cloned.get_complete());
}

#[test]
fn set_complete_reaches_every_op_sharing_the_data() {
    // FileTransform keeps the op it appends and marks its data complete once the file is
    // loaded (src/OpenColorIO/transforms/FileTransform.cpp:944-961 @ v2.5.2).
    let ops = the_no_ops();
    let kept = ops[1].clone();
    no_op_data(&kept).file_data().unwrap().set_complete();
    assert!(no_op_data(&ops[1]).file_data().unwrap().get_complete());
}

/// The clone of a no-op compared with that no-op and with an allocation no-op, as upstream's
/// `file_op` and `look_op` tests do.
fn check_clone_of_first(ops: &OpVec) {
    assert_eq!(ops.len(), 2);
    let op0 = &ops[0];
    let op1 = &ops[1];
    let cloned_op = ops[0].clone_op().unwrap();

    assert!(cloned_op.is_same_type(op0));
    assert!(!cloned_op.is_same_type(op1));
    assert!(cloned_op.is_inverse(op0));
    assert!(!cloned_op.is_inverse(op1));

    assert!(cloned_op.is_no_op().unwrap());
    assert!(!cloned_op.has_channel_crosstalk());
    assert!(cloned_op.supported_by_legacy_shader());
}

/// The scale op of upstream's `CreateGenericScaleOp` (NoOps_tests.cpp:26-30 @ v2.5.2).
fn create_generic_scale_op(ops: &mut OpVec) {
    let scale4 = [1.04, 1.05, 1.06, 1.0];
    create_scale_op(ops, &scale4, TransformDirection::Forward);
}

/// Port of `OCIO_ADD_TEST(NoOps, allocation_op)` @ v2.5.2.
#[test]
fn allocation_op() {
    let mut ops = OpVec::new();
    create_gpu_allocation_no_op(&mut ops, &lg2_allocation());
    create_generic_scale_op(&mut ops);

    check_clone_of_first(&ops);
}

/// Port of `OCIO_ADD_TEST(NoOps, file_op)` @ v2.5.2.
#[test]
fn file_op() {
    let mut ops = OpVec::new();
    create_file_no_op(&mut ops, b"");
    create_gpu_allocation_no_op(&mut ops, &lg2_allocation());

    check_clone_of_first(&ops);
}

/// Port of `OCIO_ADD_TEST(NoOps, look_op)` @ v2.5.2.
#[test]
fn look_op() {
    let mut ops = OpVec::new();
    create_look_no_op(&mut ops, b"");
    create_gpu_allocation_no_op(&mut ops, &lg2_allocation());

    check_clone_of_first(&ops);
}
