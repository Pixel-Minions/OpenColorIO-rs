// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the op list. Upstream's `Op`, `OpData`, `OpRcPtrVec` and `FinalizeOpVec` tests
//! (tests/cpu/Op_tests.cpp @ v2.5.2) all use Matrix ops, and most also other families; each
//! comes with the ops it needs. `channel_crosstalk` and `serialize` need only the Matrix op
//! and the no-ops; the others use the no-ops.

use std::sync::Arc;

use super::*;
use crate::bit_depth_utils::{F32, Uint8};
use crate::cpu_processor::BitDepthCast;
use crate::ops::matrix::MatrixOpData;
use crate::ops::matrix::matrix_op::{create_identity_matrix_op, create_matrix_op};
use crate::ops::noop::{create_file_no_op, create_look_no_op};
use crate::unit_test_log_utils::LogGuard;

const ALL_DYNAMIC_TYPES: [DynamicPropertyType; 7] = [
    DynamicPropertyType::Exposure,
    DynamicPropertyType::Contrast,
    DynamicPropertyType::Gamma,
    DynamicPropertyType::GradingPrimary,
    DynamicPropertyType::GradingRgbCurve,
    DynamicPropertyType::GradingTone,
    DynamicPropertyType::GradingHueCurve,
];

/// `n` look no-ops, named `look0`, `look1`, ...
fn looks(n: usize) -> OpVec {
    let mut ops = OpVec::new();
    for i in 0..n {
        create_look_no_op(&mut ops, format!("look{i}").as_bytes());
    }
    ops
}

/// The looks' names, in order.
fn names(ops: &OpVec) -> Vec<Vec<u8>> {
    ops.iter().map(|op| op.get_cache_id().unwrap()).collect()
}

fn same_ops(a: &[Op], b: &[Op]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| Arc::ptr_eq(x.data(), y.data()))
}

#[test]
fn cpu_op_has_no_dynamic_property_by_default() {
    // OpCPU's defaults (src/OpenColorIO/Op.cpp:29-42 @ v2.5.2), through a renderer that keeps
    // them.
    let renderers: [&dyn CpuOp; 2] = [
        &BitDepthCast::<Uint8, F32>::new(),
        &BitDepthCast::<F32, F32>::new(),
    ];
    for renderer in renderers {
        assert!(!renderer.is_dynamic());
        for type_ in ALL_DYNAMIC_TYPES {
            assert!(!renderer.has_dynamic_property(type_));
            // The message of Op::getDynamicProperty, which tests/cpu/Op_tests.cpp:271-272 @
            // v2.5.2 checks; OpCPU's is the same.
            ocio_testkit::upstream::check_throw_what(
                renderer.get_dynamic_property(type_),
                "does not implement dynamic property",
            );
        }
    }
}

#[test]
fn an_empty_list() {
    let mut ops = OpVec::new();
    assert!(ops.is_empty());
    assert_eq!(ops.get_format_metadata(), &FormatMetadataImpl::root());
    assert!(ops.is_no_op().unwrap());
    assert!(!ops.has_channel_crosstalk());
    assert!(!ops.is_dynamic());
    assert!(ops.get_cache_id().unwrap().is_empty());
    assert!(ops.validate().is_ok());
    assert!(serialize_op_vec(&ops, 4).unwrap().is_empty());
    assert!(ops.clone_ops().unwrap().is_empty());
    assert!(ops.invert().unwrap().is_empty());
    let mut finalized = ops.clone();
    finalized.finalize().unwrap();
    assert!(finalized.is_empty());
}

#[test]
fn erase_insert_and_push_back_share_the_ops() {
    let mut ops = looks(5);
    let original = ops.clone();
    assert!(same_ops(&ops, &original));

    ops.erase(1);
    assert_eq!(
        names(&ops),
        [
            b"look0".to_vec(),
            b"look2".to_vec(),
            b"look3".to_vec(),
            b"look4".to_vec()
        ]
    );

    ops.erase_range(1, 3);
    assert_eq!(names(&ops), [b"look0".to_vec(), b"look4".to_vec()]);
    assert!(Arc::ptr_eq(ops[0].data(), original[0].data()));
    assert!(Arc::ptr_eq(ops[1].data(), original[4].data()));

    // Insert a range at a position: the ops from there move right, in order.
    ops.insert(1, &original[1..3]);
    assert_eq!(
        names(&ops),
        [
            b"look0".to_vec(),
            b"look1".to_vec(),
            b"look2".to_vec(),
            b"look4".to_vec()
        ]
    );
    assert!(Arc::ptr_eq(ops[1].data(), original[1].data()));

    // Inserting nothing does nothing; inserting at the end appends.
    ops.insert(2, &[]);
    assert_eq!(ops.len(), 4);
    ops.insert(4, &original[3..4]);
    assert_eq!(names(&ops).last().unwrap(), b"look3");

    ops.push_back(original[0].clone());
    assert!(Arc::ptr_eq(ops[5].data(), original[0].data()));

    ops.get_format_metadata_mut().set_name(Some(b"kept"));
    ops.clear();
    assert!(ops.is_empty());
    assert_eq!(ops.get_format_metadata().get_name(), b"kept");
}

#[test]
fn append_shares_the_ops_and_combines_the_metadata() {
    let mut ops = looks(2);
    ops.get_format_metadata_mut()
        .add_attribute(Some(b"version"), Some(b"first"))
        .unwrap();
    let mut other = looks(3);
    other
        .get_format_metadata_mut()
        .add_attribute(Some(b"version"), Some(b"second"))
        .unwrap();
    other
        .get_format_metadata_mut()
        .add_child_element(Some(b"Description"), Some(b"desc"))
        .unwrap();

    let mut expected = ops.get_format_metadata().clone();
    expected.combine(other.get_format_metadata()).unwrap();

    ops.append(&other).unwrap();
    assert_eq!(ops.len(), 5);
    assert!(same_ops(&ops[2..], &other));
    assert_eq!(ops.get_format_metadata(), &expected);

    // `ops += ops`: upstream appends a copy.
    let copy = ops.clone();
    let mut expected = ops.get_format_metadata().clone();
    expected.combine(copy.get_format_metadata()).unwrap();
    ops.append(&copy).unwrap();
    assert_eq!(ops.len(), 10);
    assert!(same_ops(&ops[..5], &ops[5..]));
    assert_eq!(ops.get_format_metadata(), &expected);
}

#[test]
fn append_fails_after_appending_when_the_metadata_names_differ() {
    let mut ops = looks(1);
    let mut other = looks(2);
    *other.get_format_metadata_mut() = FormatMetadataImpl::new(b"Other", b"").unwrap();

    let expected_error = ops
        .get_format_metadata()
        .clone()
        .combine(other.get_format_metadata())
        .unwrap_err();
    assert_eq!(ops.append(&other).unwrap_err(), expected_error);
    // The ops are appended first.
    assert_eq!(ops.len(), 3);
}

#[test]
fn no_ops_make_a_no_op_list_without_cache_id() {
    let mut ops = looks(2);
    create_file_no_op(&mut ops, b"file.clf");
    assert!(ops.is_no_op().unwrap());
    assert!(!ops.has_channel_crosstalk());
    assert!(ops.validate().is_ok());
    // The look no-ops have cache IDs, but the list skips no-op types.
    assert!(!ops[0].get_cache_id().unwrap().is_empty());
    assert!(ops.get_cache_id().unwrap().is_empty());
}

#[test]
fn no_dynamic_property_in_a_list_of_no_ops() {
    let ops = looks(2);
    assert!(!ops.is_dynamic());
    for type_ in ALL_DYNAMIC_TYPES {
        assert!(!ops.has_dynamic_property(type_));
        // tests/cpu/Op_tests.cpp:435-437 @ v2.5.2.
        ocio_testkit::upstream::check_throw_what(
            ops.get_dynamic_property(type_),
            "Cannot find dynamic property.",
        );
    }

    let log = LogGuard::new();
    ops.validate_dynamic_properties().unwrap();
    assert!(log.empty());
}

#[test]
fn clone_ops_copies_the_ops_but_not_the_metadata() {
    let mut ops = looks(3);
    ops.get_format_metadata_mut().set_name(Some(b"list"));
    let cloned = ops.clone_ops().unwrap();
    assert_eq!(cloned.len(), 3);
    for (op, copy) in ops.iter().zip(cloned.iter()) {
        assert!(!Arc::ptr_eq(op.data(), copy.data()));
        assert_eq!(op.get_info(), copy.get_info());
        assert_eq!(op.get_cache_id(), copy.get_cache_id());
    }
    assert_eq!(cloned.get_format_metadata(), &FormatMetadataImpl::root());
}

#[test]
fn invert_reverses_and_copies_the_no_ops() {
    let mut ops = looks(3);
    create_file_no_op(&mut ops, b"file.clf");
    ops.get_format_metadata_mut().set_name(Some(b"list"));

    let inverted = ops.invert().unwrap();
    assert_eq!(inverted.len(), 4);
    for (op, inverse) in ops.iter().rev().zip(inverted.iter()) {
        assert!(!Arc::ptr_eq(op.data(), inverse.data()));
        assert!(op.is_same_type(inverse));
        assert_eq!(op.get_cache_id(), inverse.get_cache_id());
    }
    assert_eq!(inverted.get_format_metadata(), &FormatMetadataImpl::root());
}

#[test]
fn finalize_finalizes_each_op() {
    let mut ops = looks(2);
    let before = ops.clone();
    ops.finalize().unwrap();
    // The no-ops keep their data.
    assert!(same_ops(&ops, &before));
}

#[test]
fn serialize_writes_a_line_per_op() {
    // The wheel prints this text only in its debug log, which no oracle command captures yet.
    // This checks its structure: a line per op, the indent, the info, then the cache ID.
    let mut ops = looks(2);
    create_file_no_op(&mut ops, b"file.clf");

    for indent in [-3, 0, 1, 4] {
        let text = serialize_op_vec(&ops, indent).unwrap();
        let lines: Vec<&[u8]> = text.split_inclusive(|&c| c == b'\n').collect();
        assert_eq!(lines.len(), ops.len());
        let spaces = indent.max(0) as usize;
        for (line, op) in lines.iter().zip(ops.iter()) {
            assert!(line[..spaces].iter().all(|&c| c == b' '));
            assert_ne!(line[spaces], b' ');
            let info = op.get_info().as_bytes();
            assert!(line.windows(info.len()).any(|w| w == info));
            let mut tail = op.get_cache_id().unwrap();
            tail.push(b'\n');
            assert!(line.ends_with(&tail));
        }
    }
}

#[test]
fn no_op_data_makes_no_op() {
    let ops = looks(1);
    let mut result = OpVec::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        assert!(create_op_vec_from_op_data(&mut result, ops[0].data(), dir).is_err());
    }
    assert!(result.is_empty());
}

#[test]
fn display_is_the_info() {
    let ops = looks(1);
    assert_eq!(ops[0].to_string(), ops[0].get_info());
}

#[test]
fn ops_can_be_shared_between_threads() {
    // Processors share their ops between the threads that apply them.
    fn send_sync<T: Send + Sync>() {}
    send_sync::<OpData>();
    send_sync::<Op>();
    send_sync::<OpVec>();
}

#[test]
fn reference_data_makes_no_op() {
    let data: OpDataRcPtr = Arc::new(OpData::Reference(
        crate::ops::reference::ReferenceOpData::new(),
    ));
    let no_op = looks(1)[0].data().clone();
    let mut result = OpVec::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        let reference_error = create_op_vec_from_op_data(&mut result, &data, dir).unwrap_err();
        let no_op_error = create_op_vec_from_op_data(&mut result, &no_op, dir).unwrap_err();
        assert_ne!(reference_error, no_op_error);
    }
    assert!(result.is_empty());
}

/// Port of `OCIO_ADD_TEST(OpRcPtrVec, channel_crosstalk)` @ v2.5.2.
#[test]
fn channel_crosstalk() {
    let mut ops = OpVec::new();

    let mut mat = MatrixOpData::create_diagonal_matrix(1.2);
    create_matrix_op(&mut ops, mat.clone(), TransformDirection::Forward);

    assert!(!ops.has_channel_crosstalk());

    // (Upstream's first op shares `mat` and sees the change too; here it keeps its copy. The
    // list has crosstalk either way.)
    mat.set_array_value(4, 0.1);
    create_matrix_op(&mut ops, mat, TransformDirection::Forward);

    assert!(ops.has_channel_crosstalk());
}

/// Port of `OCIO_ADD_TEST(OpRcPtrVec, serialize)` @ v2.5.2.
#[test]
fn serialize() {
    // The test validates that SerializeOpVec() does not throw.

    let mut ops = OpVec::new();
    create_file_no_op(&mut ops, b"NoOp");
    create_identity_matrix_op(&mut ops);

    // Serialize not optimized OpVec i.e. contains some NoOps.
    serialize_op_vec(&ops, 0).unwrap();
}
