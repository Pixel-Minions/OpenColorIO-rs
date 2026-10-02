// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the allocation data and `CreateAllocationOps`. Upstream's `AllocationOps create`
//! (tests/cpu/ops/allocation/AllocationOp_tests.cpp @ v2.5.2) needs the Log op for its `lg2`
//! part; its other parts are here.

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

/// The parts of upstream's `AllocationOps create` (tests/cpu/ops/allocation/
/// AllocationOp_tests.cpp:12-48 @ v2.5.2) that don't need the Log op: an unknown allocation is
/// refused in both directions and adds no op; a uniform one, without variables or with two, is
/// one Fit op in both directions. The test as a whole comes with the Log op (its `lg2` part).
#[test]
fn create_allocation_ops_without_the_log_op() {
    use crate::op::OpVec;
    use crate::open_color_types::TransformDirection;
    use ocio_testkit::upstream::check_throw_what;

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
    ops.clear();
    create_allocation_ops(&mut ops, &alloc_data, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 1);
}
