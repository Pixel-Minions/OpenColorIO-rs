// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the op list: upstream's `Op`, `OpData`, `OpRcPtrVec` and `FinalizeOpVec` tests
//! (tests/cpu/Op_tests.cpp @ v2.5.2), but for `OpRcPtrVec/is_noop` and
//! `OpRcPtrVec/dynamic_property`, which need the ExposureContrast op (Phase 5). The tests
//! that aren't upstream's use the no-ops.

use std::sync::Arc;

use super::*;
use crate::bit_depth_utils::{F32, Uint8};
use crate::cpu_processor::BitDepthCast;
use crate::open_color_types::OptimizationFlags;
use crate::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use crate::ops::log::log_op::{create_log_op_from_base, create_log_op_from_parameters};
use crate::ops::matrix::MatrixOpData;
use crate::ops::matrix::matrix_op::{
    create_identity_matrix_op, create_matrix_offset_op, create_matrix_op, create_scale_op,
};
use crate::ops::noop::{create_file_no_op, create_look_no_op};
use crate::ops::range::range_op_data::RangeOpData;
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

/// Port of `Apply` (tests/cpu/Op_tests.cpp:14-20 @ v2.5.2).
fn apply(ops: &OpVec, source: &mut [f32]) {
    for op in ops.iter() {
        op.apply(source).unwrap();
    }
}

/// Port of `OCIO_ADD_TEST(FinalizeOpVec, optimize_combine)` @ v2.5.2.
#[test]
fn optimize_combine() {
    use ocio_testkit::upstream::check_close;

    let m1: [f64; 16] = [
        1.1, 0.2, 0.3, 0.4, //
        0.5, 1.6, 0.7, 0.8, //
        0.2, 0.1, 1.1, 0.2, //
        0.3, 0.4, 0.5, 1.6,
    ];

    let v1: [f64; 4] = [-0.5, -0.25, 0.25, 0.0];

    let m2: [f64; 16] = [
        1.1, -0.1, -0.1, 0.0, //
        0.1, 0.9, -0.2, 0.0, //
        0.05, 0.0, 1.1, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    let v2: [f64; 4] = [-0.2, -0.1, -0.1, -0.2];

    let source: [f32; 12] = [
        0.1, 0.2, 0.3, 0.4, //
        -0.1, -0.2, 50.0, 123.4, //
        1.0, 1.0, 1.0, 1.0,
    ];
    let error = 1e-4f32;

    let base = 10.0;
    let log_slope = [0.18, 0.18, 0.18];
    let lin_slope = [2.0, 2.0, 2.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let log_offset = [1.0, 1.0, 1.0];

    let create_log_op = |ops: &mut OpVec| {
        create_log_op_from_parameters(
            ops,
            base,
            &log_slope,
            &log_offset,
            &lin_slope,
            &lin_offset,
            TransformDirection::Forward,
        )
    };

    // Combining ops.
    {
        let mut ops = OpVec::new();
        create_matrix_offset_op(&mut ops, &m1, &v1, TransformDirection::Forward);
        create_matrix_offset_op(&mut ops, &m2, &v2, TransformDirection::Forward);
        assert_eq!(ops.len(), 2);

        // No optimize: keep both matrix ops.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(ops.len(), 2);

        // Apply ops.
        let mut tmp = source;
        apply(&ops, &mut tmp);

        // Optimize: Combine 2 matrix ops.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 1);

        // Apply ops.
        let mut tmp2 = source;
        apply(&ops, &mut tmp2);

        // Compare results.
        for i in 0..12 {
            check_close(tmp2[i], tmp[i], error);
        }
    }

    // Remove NoOp at the beginning.
    {
        let mut ops = OpVec::new();
        // NoOp.
        create_file_no_op(&mut ops, b"NoOp");
        create_identity_matrix_op(&mut ops);
        create_matrix_offset_op(&mut ops, &m1, &v1, TransformDirection::Forward);
        create_log_op(&mut ops);

        assert_eq!(ops.len(), 4);

        // No optimize: only no-ops types are removed. Keep 3 other ops.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(ops.len(), 3);

        // Apply ops.
        let mut tmp = source;
        apply(&ops, &mut tmp);

        // Optimize: remove all no-ops.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
        assert_eq!(ops[1].get_info(), "<LogOp>");

        // Apply ops.
        let mut tmp2 = source;
        apply(&ops, &mut tmp2);

        // Compare results.
        for i in 0..12 {
            check_close(tmp2[i], tmp[i], error);
        }
    }

    // remove NoOp in the middle
    {
        let mut ops = OpVec::new();
        create_matrix_offset_op(&mut ops, &m1, &v1, TransformDirection::Forward);
        // NoOp
        create_identity_matrix_op(&mut ops);
        create_file_no_op(&mut ops, b"NoOp");
        create_log_op(&mut ops);

        assert_eq!(ops.len(), 4);

        // No optimize: only no-ops types are removed.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(ops.len(), 3);

        // Apply ops.
        let mut tmp = source;
        apply(&ops, &mut tmp);

        // Optimize: remove all no ops.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
        assert_eq!(ops[1].get_info(), "<LogOp>");

        // Apply ops.
        let mut tmp2 = source;
        apply(&ops, &mut tmp2);

        // Compare results.
        for i in 0..12 {
            check_close(tmp2[i], tmp[i], error);
        }
    }

    // Remove NoOp in the end.
    {
        let mut ops = OpVec::new();
        create_matrix_offset_op(&mut ops, &m1, &v1, TransformDirection::Forward);
        create_log_op(&mut ops);
        // NoOp.
        create_identity_matrix_op(&mut ops);
        create_file_no_op(&mut ops, b"NoOp");

        assert_eq!(ops.len(), 4);

        // No optimize: only no-op types are removed.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(ops.len(), 3);

        // Apply ops.
        let mut tmp = source;
        apply(&ops, &mut tmp);

        // Optimize: remove the no op
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
        assert_eq!(ops[1].get_info(), "<LogOp>");

        // Apply ops.
        let mut tmp2 = source;
        apply(&ops, &mut tmp2);

        // Compare results.
        for i in 0..12 {
            check_close(tmp2[i], tmp[i], error);
        }
    }

    // remove several NoOp
    {
        let mut ops = OpVec::new();
        create_file_no_op(&mut ops, b"NoOp");
        create_identity_matrix_op(&mut ops);
        create_file_no_op(&mut ops, b"NoOp");
        create_matrix_offset_op(&mut ops, &m1, &v1, TransformDirection::Forward);
        create_file_no_op(&mut ops, b"NoOp");
        create_identity_matrix_op(&mut ops);
        create_log_op(&mut ops);
        create_file_no_op(&mut ops, b"NoOp");
        create_identity_matrix_op(&mut ops);

        assert_eq!(ops.len(), 9);

        // No optimize: only no-op types are removed.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(ops.len(), 5);

        // Apply ops.
        let mut tmp = source;
        apply(&ops, &mut tmp);

        // Optimize: remove all no ops.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
        assert_eq!(ops[1].get_info(), "<LogOp>");

        // Apply ops.
        let mut tmp2 = source;
        apply(&ops, &mut tmp2);

        // Compare results.
        for i in 0..12 {
            check_close(tmp2[i], tmp[i], error);
        }
    }
}

/// Port of `OCIO_ADD_TEST(Op, non_dynamic_ops)` @ v2.5.2.
#[test]
fn non_dynamic_ops() {
    let scale = [2.0, 2.0, 2.0, 1.0];

    let mut ops = OpVec::new();
    create_scale_op(&mut ops, &scale, TransformDirection::Forward);

    assert_eq!(ops.len(), 1);
    // (OCIO_REQUIRE_ASSERT(ops[0]): an op can't be null here.)

    // Test that non-dynamic ops such as matrix respond properly to dynamic
    // property requests.
    assert!(!ops[0].has_dynamic_property(DynamicPropertyType::Exposure));
    assert!(!ops[0].has_dynamic_property(DynamicPropertyType::Contrast));
    assert!(!ops[0].has_dynamic_property(DynamicPropertyType::Gamma));

    ocio_testkit::upstream::check_throw_what(
        ops[0].get_dynamic_property(DynamicPropertyType::Gamma),
        "does not implement dynamic property",
    );
}

/// The Matrix data `op` holds.
fn matrix_mut(op: &mut OpData) -> &mut MatrixOpData {
    match op {
        OpData::Matrix(data) => data,
        other => panic!("not a Matrix op: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(OpData, equality)` @ v2.5.2.
///
/// Upstream's `operator==` takes an `OpData` on its right and dispatches on its left, so each
/// comparison here is one of `OpData`s. Upstream's `op2` is `mat2` through the `OpData` base,
/// the same object: here `mat2` lives in `op2`.
#[test]
fn equality() {
    let mat1 = OpData::Matrix(MatrixOpData::create_diagonal_matrix(1.1));
    let mut op2 = mat1.clone();

    // Use the MatrixOpData::operator==().
    assert!(op2 == mat1);

    let range = OpData::Range(RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap());

    // Use the MatrixOpData::operator==().
    assert!(!(op2 == range));

    // Use the RangeOpData::operator==().
    assert!(!(range == mat1));

    // Use the OpData::operator==().
    let op1 = &range;
    assert!(!(*op1 == mat1));

    // Use the OpData::operator==().
    assert!(!(op2 == *op1));

    // Change something.

    // Use the MatrixOpData::operator==().
    assert!(op2 == mat1);

    // Use the OpData::operator==().
    assert!(op2 == mat1);

    let mat2 = matrix_mut(&mut op2);
    let value = mat2.get_offset_value(1).unwrap() + 1.0;
    mat2.set_offset_value(1, value).unwrap();

    // Use the MatrixOpData::operator==().
    assert!(!(op2 == mat1));

    // Use the OpData::operator==().
    assert!(!(op2 == mat1));
}

/// Port of `OCIO_ADD_TEST(OpRcPtrVec, erase_insert)` @ v2.5.2.
#[test]
fn erase_insert() {
    let mut ops = OpVec::new();
    let mut mat = MatrixOpData::create_diagonal_matrix(1.1);
    mat.set_id(b"First");
    create_matrix_op(&mut ops, mat, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);

    let range = RangeOpData::with_values(0.0, 1.0, 0.5, 1.5).unwrap();

    create_range_op(&mut ops, range.clone(), TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 2);

    // Test push_back.
    let mat = MatrixOpData::create_diagonal_matrix(1.3);
    create_matrix_op(&mut ops, mat, TransformDirection::Forward);

    assert_eq!(ops.len(), 3);

    // Test erase.
    ops.erase(1);

    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
    assert_eq!(ops[1].get_info(), "<MatrixOffsetOp>");

    // Test erase.
    create_log_op_from_base(&mut ops, 1.2, TransformDirection::Forward);
    create_log_op_from_base(&mut ops, 1.1, TransformDirection::Forward);

    create_range_op(&mut ops, range, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 5);

    ops.erase_range(1, 4);

    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
    assert_eq!(ops[1].get_info(), "<RangeOp>");

    // Test insert.
    let mut ops1 = ops.clone();

    assert_eq!(ops1.len(), 2);

    ops1.insert(1, &ops[0..1]);
    assert_eq!(ops1.len(), 3);
    assert_eq!(ops1[0].get_info(), "<MatrixOffsetOp>");
    assert_eq!(ops1[1].get_info(), "<MatrixOffsetOp>");
    assert_eq!(ops1[2].get_info(), "<RangeOp>");

    // Test operator +=.
    let mut ops2 = ops.clone();
    assert_eq!(ops2.len(), 2);

    // (`ops2 += ops2`: upstream appends a copy.)
    let copy = ops2.clone();
    ops2.append(&copy).unwrap();

    assert_eq!(ops2.len(), 4);
    assert_eq!(ops2[0].get_info(), "<MatrixOffsetOp>");
    assert_eq!(ops2[1].get_info(), "<RangeOp>");
    assert_eq!(ops2[2].get_info(), "<MatrixOffsetOp>");
    assert_eq!(ops2[3].get_info(), "<RangeOp>");
}

/// Port of `OCIO_ADD_TEST(OpRcPtrVec, clone_invert)` @ v2.5.2.
#[test]
fn clone_invert() {
    let mut ops = OpVec::new();

    create_look_no_op(&mut ops, b"look");

    let params = vec![1.001];
    let gamma = GammaOpData::new(
        GammaStyle::BasicFwd,
        params.clone(),
        params.clone(),
        params.clone(),
        params,
    );
    create_gamma_op(&mut ops, gamma, TransformDirection::Forward);

    create_log_op_from_base(&mut ops, 2., TransformDirection::Forward);

    assert_eq!(ops.len(), 3);

    // Test the clone() method.

    let cloned = ops.clone_ops().unwrap();
    assert_eq!(cloned.len(), 3);

    // (Upstream compares the ops' addresses; an op here is its shared data.)
    assert!(!Arc::ptr_eq(ops[0].data(), cloned[0].data()));
    assert!(!Arc::ptr_eq(ops[1].data(), cloned[1].data()));
    assert!(!Arc::ptr_eq(ops[2].data(), cloned[2].data()));

    assert_eq!(ops[0].get_info(), cloned[0].get_info());
    assert_eq!(ops[1].get_info(), cloned[1].get_info());
    assert_eq!(ops[2].get_info(), cloned[2].get_info());

    // Test the invert() method.

    let inverted = ops.invert().unwrap();
    assert_eq!(inverted.len(), 3);

    for op1 in ops.iter() {
        for op2 in cloned.iter() {
            assert!(!Arc::ptr_eq(op1.data(), op2.data()));
        }
    }

    // Test the Log.
    let inv = &inverted[0];
    assert!(ops[2].is_inverse(inv));

    // Test the Gamma.
    let inv = &inverted[1];
    assert!(ops[1].is_inverse(inv));

    assert_eq!(ops[0].get_info(), inverted[2].get_info());
    assert_eq!(ops[1].get_info(), inverted[1].get_info());
    assert_eq!(ops[2].get_info(), inverted[0].get_info());
}
