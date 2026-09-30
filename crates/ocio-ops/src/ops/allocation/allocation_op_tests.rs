// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the allocation data. Upstream's `AllocationOps create`
//! (tests/cpu/ops/allocation/AllocationOp_tests.cpp @ v2.5.2) tests `CreateAllocationOps`,
//! which comes with the Matrix and Log ops.

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
