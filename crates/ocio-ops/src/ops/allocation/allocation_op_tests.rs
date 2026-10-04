// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the allocation data, and of `CreateAllocationOps`: upstream's `AllocationOps create`
//! (tests/cpu/ops/allocation/AllocationOp_tests.cpp @ v2.5.2).

use super::*;

/// Variables that exercise `%.7g`: integers, rounding at the 7th digit, exponents, extremes,
/// infinities and NaNs of both signs.
const VARS: [f32; 15] = [
    -8.0,
    8.0,
    0.0,
    -0.0,
    0.1,
    1.0e-5,
    123456.78,
    1234567.9,
    16777216.0,
    3.4028235e38,
    1.0e-45,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NAN,
    -f32::NAN,
];

#[test]
fn the_default_is_uniform_without_variables() {
    let data = AllocationData::default();
    assert_eq!(data.allocation, Allocation::Uniform);
    assert!(data.vars.is_empty());
}

#[test]
fn the_cache_id_prints_the_variables_with_7_digits() {
    // The names are those upstream's config tests read back (tests/cpu/ColorSpace_tests.cpp:
    // 257, 323 @ v2.5.2). Each variable is `os << float` with precision 7: the C runtime's
    // `%.7g` of the value promoted to double (C++ `num_put`).
    for (allocation, name) in [(Allocation::Uniform, "uniform"), (Allocation::Lg2, "lg2")] {
        for count in [0, 1, 2, 3, VARS.len()] {
            let data = AllocationData {
                allocation,
                vars: VARS[..count].to_vec(),
            };
            let mut expected = format!("{name} ");
            for &var in &data.vars {
                expected += &ocio_testkit::crt::format_f64("%.7g", f64::from(var));
                expected += " ";
            }
            assert_eq!(data.get_cache_id(), expected, "{data:?}");
            assert_eq!(data.to_string(), expected);
        }
    }
}

#[test]
fn an_unknown_allocation_has_its_own_name() {
    let data = AllocationData {
        allocation: Allocation::Unknown,
        vars: vec![1.0],
    };
    let id = data.get_cache_id();
    let name = id.split(' ').next().unwrap();
    assert!(!name.is_empty());
    assert_ne!(name, "uniform");
    assert_ne!(name, "lg2");
    assert_eq!(
        id,
        format!("{name} {} ", ocio_testkit::crt::format_f64("%.7g", 1.0))
    );
}

/// Port of `OCIO_ADD_TEST(AllocationOps, create)` @ v2.5.2. The wheel is built with
/// `OCIO_USE_SSE2`, so the test keeps its `#else` tolerance.
#[test]
fn create() {
    use crate::op::OpVec;
    use crate::open_color_types::{OptimizationFlags, TransformDirection};
    use ocio_testkit::upstream::{check_close, check_throw_what};

    let mut ops = OpVec::new();
    let mut alloc_data = AllocationData {
        allocation: Allocation::Unknown,
        ..AllocationData::default()
    };
    check_throw_what(
        create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Forward),
        "Unsupported Allocation Type",
    );
    assert_eq!(ops.len(), 0);
    check_throw_what(
        create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Inverse),
        "Unsupported Allocation Type",
    );
    assert_eq!(ops.len(), 0);

    alloc_data.allocation = Allocation::Uniform;
    // No allocation data leads to identity, identity transform will be created.
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    ops.clear();
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 1);

    // adding data to avoid identity. Fit transform will be created (if valid).
    alloc_data.vars.push(0.0f32);
    alloc_data.vars.push(10.0f32);
    ops.clear();
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    let forward_fit_op = ops[0].clone_op().unwrap();
    ops.clear();
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 1);
    ops.clear();

    alloc_data.allocation = Allocation::Lg2;

    // default is not identity
    alloc_data.vars.clear();
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);
    // second op is a fit transform
    assert!(forward_fit_op.is_same_type(&ops[1]));
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 2);
    let default_log_op = ops[0].clone_op().unwrap();

    let error = 2e-5f32;
    const NB_PIXELS: usize = 3;
    let src: [f32; NB_PIXELS * 4] = [
        0.16, 0.2, 0.3, 0.4, //
        -0.16, -0.2, 32.0, 123.4, //
        1.0, 1.0, 1.0, 1.0,
    ];

    let dst_log: [f32; NB_PIXELS * 4] = [
        -2.64385629,
        -2.32192802,
        -1.73696554,
        0.4, //
        -126.0,
        -126.0,
        5.0,
        123.4, //
        0.0,
        0.0,
        0.0,
        1.0,
    ];

    let dst_fit: [f32; NB_PIXELS * 4] = [
        0.635, 0.6375, 0.64375, 0.4, //
        0.615, 0.6125, 2.625, 123.4, //
        0.6875, 0.6875, 0.6875, 1.0,
    ];

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();

    for idx in 0..NB_PIXELS * 4 {
        check_close(dst_log[idx], tmp[idx], error);
    }

    let mut tmp = src;

    ops[1].apply(&mut tmp).unwrap();

    for idx in 0..NB_PIXELS * 4 {
        check_close(dst_fit[idx], tmp[idx], error);
    }

    ops.clear();

    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(default_log_op.is_inverse(&ops[1]));

    ops.clear();

    // adding data to target identity, Log op and identity are created
    alloc_data.vars.push(0.0f32);
    alloc_data.vars.push(1.0f32);

    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);
    // Identity is removed.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(default_log_op.is_same_type(&ops[0]));
    ops.clear();

    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 2);
    // Identity is removed.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(default_log_op.is_same_type(&ops[0]));
    ops.clear();

    // change log intercept
    alloc_data.vars.push(10.0f32);
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);

    let mut tmp = src;

    let dst_log_shift: [f32; NB_PIXELS * 4] = [
        3.34482837, 3.35049725, 3.36457253, 0.4, //
        3.29865813, 3.29278183, 5.39231730, 123.4, //
        3.45943165, 3.45943165, 3.45943165, 1.0,
    ];

    ops[0].apply(&mut tmp).unwrap();

    for idx in 0..NB_PIXELS * 4 {
        check_close(dst_log_shift[idx], tmp[idx], error);
    }
}
