// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/range/RangeOp_tests.cpp` @ v2.5.2: the tests of the op. Its tests of
//! `CreateRangeTransform` and `BuildRangeOp` (`create_transform`, and
//! `RangeTransform.no_clamp_converts_to_matrix`) need the `RangeTransform`, and come with the
//! transforms (WP 1.8).

use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;

/// `g_error` (RangeOp_tests.cpp:15-18 @ v2.5.2).
const G_ERROR: f32 = 1e-7;

/// Port of `OCIO_ADD_TEST(RangeOp, apply_arbitrary)` @ v2.5.2.
#[test]
fn apply_arbitrary() {
    let range = RangeOpData::with_values(-0.101, 0.95, 0.194, 1.001).unwrap();

    let mut ops = OpVec::new();
    create_range_op(&mut ops, range, TransformDirection::Forward).unwrap();
    let r = &mut ops[0];
    r.validate().unwrap();
    r.finalize().unwrap();

    let mut image: [f32; 12] = [
        -0.50, 0.25, 0.50, 0.0, //
        0.75, 1.00, 1.25, 1.0, //
        1.25, 1.50, 1.75, 0.0,
    ];

    r.apply(&mut image).unwrap();

    let expected: [f32; 12] = [
        0.194,
        0.4635119438,
        0.6554719806,
        0.0,
        0.8474320173,
        1.001,
        1.001,
        1.0,
        1.001,
        1.001,
        1.001,
        0.0,
    ];
    for (&value, &expected) in image.iter().zip(&expected) {
        check_close(value, expected, G_ERROR);
    }
}

/// Port of `OCIO_ADD_TEST(RangeOp, combining)` @ v2.5.2.
#[test]
fn combining() {
    let mut ops = OpVec::new();

    create_range_op_from_values(&mut ops, 0., 0.5, 0.5, 1.0, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    ops[0].validate().unwrap();
    create_range_op_from_values(&mut ops, 0., 1., 0.5, 1.5, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);
    ops[1].validate().unwrap();

    let op1 = ops[1].clone();

    let op0 = ops[0].clone();
    op0.combine_with(&mut ops, &op1).unwrap();
    assert_eq!(ops.len(), 3);
}

/// Port of `OCIO_ADD_TEST(RangeOp, combining_with_inverse)` @ v2.5.2.
#[test]
fn combining_with_inverse() {
    let mut ops = OpVec::new();

    create_range_op_from_values(&mut ops, 0., 1., 0.5, 1.5, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    ops[0].validate().unwrap();
    create_range_op_from_values(&mut ops, 0., 1., 0.5, 1.5, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 2);
    ops[1].validate().unwrap();

    let op1 = ops[1].clone();

    let op0 = ops[0].clone();
    check_throw_what(
        op0.combine_with(&mut ops, &op1),
        "Op::finalize has to be called",
    );

    check_throw_what(
        ops[0].can_combine_with(&op1),
        "Op::finalize has to be called",
    );
    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);
    // Upstream's `op1` is the op that `finalize` changes in place; here finalizing gives the
    // op in the list new data, so `op1` is taken again.
    let op1 = ops[1].clone();
    ops[0].can_combine_with(&op1).unwrap();
    let op0 = ops[0].clone();
    op0.combine_with(&mut ops, &op1).unwrap();

    assert_eq!(ops.len(), 3);
}

/// Port of `OCIO_ADD_TEST(RangeOp, computed_identifier)` @ v2.5.2.
#[test]
fn computed_identifier() {
    let mut ops = OpVec::new();

    create_range_op_from_values(&mut ops, 0., 0.5, 0.5, 1.0, TransformDirection::Forward).unwrap();
    create_range_op_from_values(&mut ops, 0., 0.5, 0.5, 1.0, TransformDirection::Forward).unwrap();
    create_range_op_from_values(&mut ops, 0.1, 1., 0.3, 1.9, TransformDirection::Forward).unwrap();
    create_range_op_from_values(&mut ops, 0.1, 1., 0.3, 1.9, TransformDirection::Inverse).unwrap();

    assert_eq!(ops.len(), 4);

    let cache_id0 = ops[0].get_cache_id();
    let cache_id1 = ops[1].get_cache_id();
    let cache_id2 = ops[2].get_cache_id();
    let cache_id3 = ops[3].get_cache_id();
    assert!(cache_id0 == cache_id1);
    assert!(cache_id0 != cache_id2);
    assert!(cache_id1 != cache_id2);
    assert!(cache_id2 != cache_id3);

    create_range_op_from_values(&mut ops, 0.1, 1., 0.3, 1.90001, TransformDirection::Forward)
        .unwrap();

    assert_eq!(ops.len(), 5);
    let cache_id4 = ops[4].get_cache_id();
    assert!(cache_id2 != cache_id4);
    assert!(cache_id3 != cache_id4);
}
