// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/log/LogOp_tests.cpp` @ v2.5.2: the tests of the op. Its test of
//! `CreateLogTransform` (`create_transform`) needs the Log transforms, and comes with them
//! (WP 1.8).
//!
//! The wheel is built with `OCIO_USE_SSE2`, so the ports keep the `#if OCIO_USE_SSE2` branches.

use ocio_testkit::upstream::check_close;

use super::*;
use crate::open_color_types::OptimizationFlags;

/// `OCIO_CHECK_CLOSE(x, y, tol)` with `float` values and a `double` tolerance: the difference
/// in `float`, compared in `double` (tests/testutils/UnitTest.h:194-212 @ v2.5.2).
#[track_caller]
fn check_close_f32_f64(x: f32, y: f32, tol: f64) {
    let passes = f64::from((x - y).abs()) < tol;
    assert!(
        passes,
        "OCIO_CHECK_CLOSE failed: abs({x:e} - {y:e}) < {tol:e}"
    );
}

/// Port of `OCIO_ADD_TEST(LogOp, lin_to_log)` @ v2.5.2.
#[test]
fn lin_to_log() {
    let base = 10.0;
    let log_slope = [0.18, 0.18, 0.18];
    let lin_slope = [2.0, 2.0, 2.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let log_offset = [1.0, 1.0, 1.0];

    let mut data: [f32; 8] = [0.01, 0.1, 1.0, 1.0, 10.0, 100.0, 1000.0, 1.0];

    let result: [f32; 8] = [
        0.8342526242885725,
        0.90588182584953925,
        1.057999473052105462,
        1.0,
        1.23457529033568797,
        1.41422447595451795,
        1.59418930777214063,
        1.0,
    ];

    let mut ops = OpVec::new();
    create_log_op_from_parameters(
        &mut ops,
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Forward,
    );

    // One operator has been created.
    assert_eq!(ops.len(), 1);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Validate properties.
    let op_cache = ops[0].get_cache_id().unwrap();
    assert_ne!(op_cache.len(), 0);
    assert!(!ops[0].is_no_op().unwrap());
    assert!(!ops[0].has_channel_crosstalk());

    // Apply the result.
    for op in ops.iter() {
        op.apply(&mut data).unwrap();
    }

    for i in 0..8 {
        check_close_f32_f64(data[i], result[i], 1.0e-3);
    }
}

/// Port of `OCIO_ADD_TEST(LogOp, log_to_lin)` @ v2.5.2.
#[test]
fn log_to_lin() {
    let base = 10.0;
    let log_slope = [0.18, 0.18, 0.18];
    let lin_slope = [2.0, 2.0, 2.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let log_offset = [1.0, 1.0, 1.0];

    let mut data: [f32; 8] = [
        0.8342526242885725,
        0.90588182584953925,
        1.057999473052105462,
        1.0,
        1.23457529033568797,
        1.41422447595451795,
        1.59418930777214063,
        1.0,
    ];

    let result: [f32; 8] = [0.01, 0.1, 1.0, 1.0, 10.0, 100.0, 1000.0, 1.0];

    let mut ops = OpVec::new();
    create_log_op_from_parameters(
        &mut ops,
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Inverse,
    );

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Apply the result.
    for op in ops.iter() {
        op.apply(&mut data).unwrap();
    }

    for i in 0..8 {
        check_close(data[i], result[i], 2.0e-3f32);
    }
}

/// Port of `OCIO_ADD_TEST(LogOp, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let mut base = 10.0;
    let log_slope = [0.5, 0.5, 0.5];
    let lin_slope = [2.0, 2.0, 2.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let log_offset = [1.0, 1.0, 1.0];
    let log_slope2 = [0.5, 1.0, 1.5];

    let mut ops = OpVec::new();
    let create = |ops: &mut OpVec, base, log_slope: &[f64; 3], dir| {
        create_log_op_from_parameters(
            ops,
            base,
            log_slope,
            &log_offset,
            &lin_slope,
            &lin_offset,
            dir,
        );
    };
    create(&mut ops, base, &log_slope, TransformDirection::Forward);
    create(&mut ops, base, &log_slope, TransformDirection::Inverse);

    base += 1.0;
    create(&mut ops, base, &log_slope, TransformDirection::Inverse);
    create(&mut ops, base, &log_slope, TransformDirection::Forward);

    create(&mut ops, base, &log_slope2, TransformDirection::Inverse);
    create(&mut ops, base, &log_slope2, TransformDirection::Forward);

    assert_eq!(ops.len(), 6);
    let op0 = ops[0].clone();
    let op1 = ops[1].clone();
    let op2 = ops[2].clone();
    let op3 = ops[3].clone();
    let op5 = ops[5].clone();

    assert!(ops[0].is_same_type(&op1));
    assert!(ops[0].is_same_type(&op2));
    let op3_cloned = ops[3].clone_op();
    assert!(ops[0].is_same_type(&op3_cloned));

    assert!(!ops[0].is_inverse(&op0));
    assert!(ops[0].is_inverse(&op1));
    assert!(!ops[0].is_inverse(&op2));
    assert!(!ops[0].is_inverse(&op3));

    assert!(ops[1].is_inverse(&op0));
    assert!(!ops[1].is_inverse(&op2));
    assert!(!ops[1].is_inverse(&op3));

    assert!(!ops[2].is_inverse(&op2));
    assert!(ops[2].is_inverse(&op3));

    assert!(!ops[3].is_inverse(&op3));

    // When r, g & b are not equal, ops are not considered inverse even though they are.
    assert!(!ops[4].is_inverse(&op5));

    let result: [f32; 12] = [
        0.01, 0.1, 1.0, 1.0, //
        1.0, 10.0, 100.0, 1.0, //
        1000.0, 1.0, 0.5, 1.0,
    ];
    let mut data = result;

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    ops[0].apply(&mut data).unwrap();
    // Note: Skip testing alpha channels.
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        assert_ne!(data[i], result[i]);
    }

    ops[1].apply(&mut data).unwrap();

    // #if OCIO_USE_SSE2 == 0 ... #else
    let error = 1e-2f32;

    for i in 0..12 {
        check_close(data[i], result[i], error);
    }
}

/// Port of `OCIO_ADD_TEST(LogOp, cache_id)` @ v2.5.2.
#[test]
fn cache_id() {
    let base = 10.0;
    let log_slope = [0.18, 0.18, 0.18];
    let lin_slope = [2.0, 2.0, 2.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let mut log_offset = [1.0, 1.0, 1.0];

    let mut ops = OpVec::new();
    create_log_op_from_parameters(
        &mut ops,
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Forward,
    );
    log_offset[0] += f64::from(1.0f32);
    create_log_op_from_parameters(
        &mut ops,
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Forward,
    );
    log_offset[0] -= f64::from(1.0f32);
    create_log_op_from_parameters(
        &mut ops,
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Forward,
    );

    // 3 operators have been created
    assert_eq!(ops.len(), 3);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    let op_cache_id0 = ops[0].get_cache_id().unwrap();
    let op_cache_id1 = ops[1].get_cache_id().unwrap();
    let op_cache_id2 = ops[2].get_cache_id().unwrap();
    assert_eq!(op_cache_id0, op_cache_id2);
    assert_ne!(op_cache_id0, op_cache_id1);
}
