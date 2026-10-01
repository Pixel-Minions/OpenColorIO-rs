// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/exponent/ExponentOp_tests.cpp` @ v2.5.2, but `create_transform`,
//! which needs the transforms (1.8). `tests/exponent_oracle.rs` checks the renderer against
//! the wheel, and the optimizer's combinations through the processor's cache ID.

use ocio_testkit::upstream::{check_close, check_equal, check_throw_what};

use super::*;
use crate::format_metadata::METADATA_DESCRIPTION;
use crate::open_color_types::OptimizationFlags;
use crate::ops::noop::create_file_no_op;

/// Port of `OCIO_ADD_TEST(ExponentOp, value)` @ v2.5.2.
#[test]
fn value() {
    let exp1 = [1.2, 1.3, 1.4, 1.5];

    let mut ops = OpVec::new();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Inverse).unwrap();
    check_equal(ops.len(), 2);

    ops.finalize().unwrap();

    let error = 1e-6f32;

    let source = [0.1f32, 0.3, 0.9, 0.5];

    let result1 = [0.0630957261f32, 0.209053621, 0.862858355, 0.353553385];

    let mut tmp = source;
    ops[0].apply(&mut tmp).unwrap();

    for i in 0..4 {
        check_close(tmp[i], result1[i], error);
    }

    ops[1].apply(&mut tmp).unwrap();
    for i in 0..4 {
        check_close(tmp[i], source[i], error);
    }
}

/// Port of `ValidateOp` (ExponentOp_tests.cpp:47-57 @ v2.5.2).
fn validate_op(source: &[f32; 4], op: &Op, result: &[f32; 4], error: f32) {
    let mut tmp = *source;
    op.apply(&mut tmp).unwrap();

    for i in 0..4 {
        check_close(tmp[i], result[i], error);
    }
}

/// Port of `OCIO_ADD_TEST(ExponentOp, value_limits)` @ v2.5.2.
#[test]
fn value_limits() {
    let exp1 = [0.0, 2.0, -2.0, 1.5];

    let mut ops = OpVec::new();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();

    ops.finalize().unwrap();

    let error = 1e-6f32;

    let source1 = [1.0f32, 1.0, 1.0, 1.0];
    let result1 = [1.0f32, 1.0, 1.0, 1.0];
    validate_op(&source1, &ops[0], &result1, error);

    let source2 = [2.0f32, 2.0, 2.0, 2.0];
    let result2 = [1.0f32, 4.0, 0.25, 2.82842708];
    validate_op(&source2, &ops[0], &result2, error);

    let source3 = [-2.0f32, -2.0, 1.0, -2.0];
    let result3 = [1.0f32, 0.0, 1.0, 0.0];
    validate_op(&source3, &ops[0], &result3, error);

    let source4 = [0.0f32, 0.0, 1.0, 0.0];
    let result4 = [1.0f32, 0.0, 1.0, 0.0];
    validate_op(&source4, &ops[0], &result4, error);
}

/// Port of `OCIO_ADD_TEST(ExponentOp, combining)` @ v2.5.2.
#[test]
fn combining() {
    let error = 1e-6f32;
    {
        let exp1 = [2.0, 2.0, 2.0, 1.0];
        let exp2 = [1.2, 1.2, 1.2, 1.0];

        let mut exp_data1 = ExponentOpData::from_values(&exp1);
        let mut exp_data2 = ExponentOpData::from_values(&exp2);
        exp_data1.set_name(b"Exp1");
        exp_data1.set_id(b"ID1");
        exp_data1
            .get_format_metadata_mut()
            .add_child_element(Some(METADATA_DESCRIPTION), Some(b"First exponent"))
            .unwrap();
        exp_data2.set_name(b"Exp2");
        exp_data2.set_id(b"ID2");
        exp_data2
            .get_format_metadata_mut()
            .add_child_element(Some(METADATA_DESCRIPTION), Some(b"Second exponent"))
            .unwrap();
        exp_data2
            .get_format_metadata_mut()
            .add_attribute(Some(b"Attrib"), Some(b"value"))
            .unwrap();

        let mut ops = OpVec::new();
        create_exponent_op(&mut ops, exp_data1, TransformDirection::Forward).unwrap();
        create_exponent_op(&mut ops, exp_data2, TransformDirection::Forward).unwrap();
        assert_eq!(ops.len(), 2);

        ops.finalize().unwrap();

        let op1 = ops[1].clone();

        let source = [0.9f32, 0.4, 0.1, 0.5];
        let result = [0.776572466f32, 0.110903174, 0.00398107106, 0.5];

        let mut tmp = source;
        ops[0].apply(&mut tmp).unwrap();
        ops[1].apply(&mut tmp).unwrap();

        for i in 0..4 {
            check_close(tmp[i], result[i], error);
        }

        let mut combined = OpVec::new();
        ops[0].combine_with(&mut combined, &op1).unwrap();
        check_equal(combined.len(), 1);

        let combined_data = combined[0].data();

        // Check metadata of combined op.
        check_equal(combined_data.get_name(), &b"Exp1 + Exp2"[..]);
        check_equal(combined_data.get_id(), &b"ID1 + ID2"[..]);
        let metadata = combined_data.get_format_metadata();
        assert_eq!(metadata.get_num_children_elements(), 2);
        let child0 = metadata.get_child_element(0).unwrap();
        check_equal(child0.get_element_name(), METADATA_DESCRIPTION);
        check_equal(child0.get_element_value(), &b"First exponent"[..]);
        let child1 = metadata.get_child_element(1).unwrap();
        check_equal(child1.get_element_name(), METADATA_DESCRIPTION);
        check_equal(child1.get_element_value(), &b"Second exponent"[..]);
        // 3 attributes: name, id and Attrib.
        check_equal(metadata.get_num_attributes(), 3);
        let attribs = metadata.get_attributes();
        check_equal(attribs[2].0.as_slice(), &b"Attrib"[..]);
        check_equal(attribs[2].1.as_slice(), &b"value"[..]);

        combined.finalize().unwrap();

        let mut tmp2 = source;
        combined[0].apply(&mut tmp2).unwrap();

        for i in 0..4 {
            check_close(tmp2[i], result[i], error);
        }
    }

    {
        let exp1 = [1.037289, 1.019015, 0.966082, 1.0];

        let mut ops = OpVec::new();
        create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
        create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Inverse).unwrap();

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        let op1 = ops[1].clone();

        let mut combined = OpVec::new();
        ops[0].combine_with(&mut combined, &op1).unwrap();
        check_equal(combined.is_empty(), true);
    }

    {
        let exp1 = [1.037289, 1.019015, 0.966082, 1.0];

        let mut ops = OpVec::new();
        create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
        create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
        create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();

        ops.finalize().unwrap();
        check_equal(ops.len(), 3);

        let source = [0.1f32, 0.5, 0.9, 0.5];
        let result = [0.0765437484f32, 0.480251998, 0.909373641, 0.5];

        let mut tmp = source;
        ops[0].apply(&mut tmp).unwrap();
        ops[1].apply(&mut tmp).unwrap();
        ops[2].apply(&mut tmp).unwrap();

        for i in 0..4 {
            check_close(tmp[i], result[i], error);
        }

        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        check_equal(ops.len(), 1);

        tmp = source;
        ops[0].apply(&mut tmp).unwrap();

        for i in 0..4 {
            check_close(tmp[i], result[i], error);
        }
    }
}

/// Port of `OCIO_ADD_TEST(ExponentOp, throw_create)` @ v2.5.2.
#[test]
fn throw_create() {
    let exp1 = [0.0, 1.3, 1.4, 1.5];

    let mut ops = OpVec::new();

    check_throw_what(
        create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Inverse),
        "Cannot apply 0.0 exponent in the inverse",
    );
}

/// Port of `OCIO_ADD_TEST(ExponentOp, can_combine_with)` @ v2.5.2.
#[test]
fn can_combine_with() {
    let exp1 = [0.0, 1.3, 1.4, 1.5];

    let mut ops = OpVec::new();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
    create_file_no_op(&mut ops, b"NoOp");
    assert_eq!(ops.len(), 2);
    let op1 = ops[1].clone();

    assert!(!ops[0].can_combine_with(&op1).unwrap());
    let op0 = ops[0].clone();
    check_throw_what(
        op0.combine_with(&mut ops, &op1),
        "ExponentOp: canCombineWith must be checked",
    );
}

/// Port of `OCIO_ADD_TEST(ExponentOp, noop)` @ v2.5.2.
#[test]
fn noop() {
    let exp1 = [1.0, 1.0, 1.0, 1.0];

    // CreateExponentOp will create a NoOp
    let mut ops = OpVec::new();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Inverse).unwrap();

    assert_eq!(ops.len(), 2);
    assert!(ops[0].is_no_op());
    assert!(ops[1].is_no_op());

    // Optimize it.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    check_equal(ops.len(), 0);
}

/// Port of `OCIO_ADD_TEST(ExponentOp, cache_id)` @ v2.5.2.
#[test]
fn cache_id() {
    let exp1 = [2.0, 2.1, 3.0, 3.1];
    let exp2 = [4.0, 4.1, 5.0, 5.1];

    let mut ops = OpVec::new();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();
    create_exponent_op_from_values(&mut ops, &exp2, TransformDirection::Forward).unwrap();
    create_exponent_op_from_values(&mut ops, &exp1, TransformDirection::Forward).unwrap();

    check_equal(ops.len(), 3);

    ops.validate().unwrap();

    let op_cache_id0 = ops[0].get_cache_id();
    let op_cache_id1 = ops[1].get_cache_id();
    let op_cache_id2 = ops[2].get_cache_id();

    check_equal(&op_cache_id0, &op_cache_id2);
    assert_ne!(op_cache_id0, op_cache_id1);
}
