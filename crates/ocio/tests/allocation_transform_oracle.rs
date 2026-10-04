// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `AllocationTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the allocation and the variables only when there are
//!   variables, each `float` with 6 significant digits, or 16 after a MatrixTransform in the
//!   same group (I-73); its validation, every message; the binding gives it no `equals()`;
//! - the raw config's processor of each, in both directions: `BuildAllocationOp` and
//!   `CreateAllocationOps`, a Fit Matrix op for the uniform allocation, a Log op and a fit (in
//!   the reverse order inverse) for `lg2`, through `createGroupTransform()`.
//!
//! The binding's `setVars` takes 2 or 3 variables (src/bindings/python/transforms/
//! PyAllocationTransform.cpp:23-30 @ v2.5.2), so the specs set 2 or 3, or none. The C++
//! `setVars` (src/OpenColorIO/transforms/AllocationTransform.cpp:143-157) takes any number,
//! so the text of an allocation with 1 variable, or more than 3, is unpinned: the wheel can't
//! build one. Validation refuses those counts for both allocations (98-111), so no processor
//! holds one.

mod common;

use common::transforms::{Case, check_processors, check_text, direction_spec, group};
use ocio::{Allocation, AllocationTransform, MatrixTransform, TransformDirection};
use ocio_testkit::transform_text::f64_spec;
use serde_json::json;

use TransformDirection::{Forward, Inverse};

/// The allocation, as the binding names it.
fn allocation_name(allocation: Allocation) -> &'static str {
    match allocation {
        Allocation::Unknown => "ALLOCATION_UNKNOWN",
        Allocation::Uniform => "ALLOCATION_UNIFORM",
        Allocation::Lg2 => "ALLOCATION_LG2",
    }
}

/// The case of an allocation with `vars` (none, 2 or 3), built through the setters (the
/// binding's constructor validates).
fn allocation_case(allocation: Allocation, vars: &[f32], dir: TransformDirection) -> Case {
    let mut port = AllocationTransform::new();
    port.set_allocation(allocation);
    port.set_direction(dir);
    let mut calls = vec![
        json!(["setAllocation", {"enum": allocation_name(allocation)}]),
        json!(["setDirection", direction_spec(dir)]),
    ];
    if !vars.is_empty() {
        port.set_vars(vars);
        let values: Vec<_> = vars.iter().map(|&v| f64_spec(f64::from(v))).collect();
        calls.push(json!(["setVars", values]));
    }
    Case::new(
        format!("{allocation:?} {vars:?} {dir:?}"),
        json!({"class": "AllocationTransform", "calls": calls}),
        port,
    )
}

/// Floats that print in their own way: quiet NaNs of both signs, the infinities, both zeros,
/// subnormals, the extremes, values at the edge of 6 significant digits, ordinary ones.
fn special_floats() -> Vec<f32> {
    let mut values: Vec<f32> = [0x7fc0_0000u32, 0xffc0_0000, 0x7fc0_beef]
        .map(f32::from_bits)
        .to_vec();
    values.extend([
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::MIN,
        0.1,
        1.0 / 3.0,
        -8.0,
        8.0,
        -10.0,
        6.0,
        123_456.5,
        999_999.5,
        1_234_567.0,
        16_777_216.0,
        1e-7,
        0.5,
    ]);
    values
}

/// The cases: every allocation, with no variables, 2 and 3, in both directions, the special
/// floats as variables, and each validation error.
fn cases() -> Vec<Case> {
    let mut cases = vec![Case::new(
        "the default",
        json!({"class": "AllocationTransform"}),
        AllocationTransform::new(),
    )];
    let specials = special_floats();
    for allocation in [Allocation::Uniform, Allocation::Lg2, Allocation::Unknown] {
        for dir in [Forward, Inverse] {
            cases.push(allocation_case(allocation, &[], dir));
            cases.push(allocation_case(allocation, &[-8.0, 8.0], dir));
            cases.push(allocation_case(allocation, &[-10.0, 6.0, 0.0001], dir));
            cases.push(allocation_case(allocation, &[0.0, 0.0], dir));
        }
    }
    for k in 0..specials.len() {
        let v = |i: usize| specials[(k + i) % specials.len()];
        for allocation in [Allocation::Uniform, Allocation::Lg2] {
            cases.push(allocation_case(allocation, &[v(0), v(1)], Forward));
            cases.push(allocation_case(allocation, &[v(0), v(5), v(11)], Inverse));
        }
    }
    cases
}

/// Groups that show a MatrixTransform's precision on the variables after it (I-73).
fn group_cases() -> Vec<Case> {
    let vars = [1.0f32 / 3.0, 2.0 / 3.0, 0.1];
    let allocation = allocation_case(Allocation::Lg2, &vars, Forward);
    let matrix = Case::new(
        "a matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    vec![
        group(
            "an allocation, a matrix, the allocation",
            Forward,
            &[allocation.clone(), matrix.clone(), allocation.clone()],
        ),
        group(
            "an allocation in a group after a matrix",
            Inverse,
            &[
                group("a matrix", Forward, std::slice::from_ref(&matrix)),
                group("an allocation", Inverse, std::slice::from_ref(&allocation)),
            ],
        ),
    ]
}

#[test]
fn text_and_validation_match_the_wheel() {
    let mut cases = cases();
    cases.extend(group_cases());
    // The binding has no equals() for the class.
    let pairs = vec![(0, 0), (0, 1), (1, 0)];
    check_text(&cases, &pairs);
}

#[test]
fn processors_match_the_wheel() {
    let mut cases = cases();
    cases.extend(group_cases());
    check_processors(&cases);
}
