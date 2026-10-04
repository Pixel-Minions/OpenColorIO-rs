// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the allocation transform: `tests/cpu/transforms/AllocationTransform_tests.cpp` @
//! v2.5.2. The text, the validation and the ops built are compared with the wheel's in
//! `tests/allocation_transform_oracle.rs`.

use super::*;

/// Port of `OCIO_ADD_TEST(AllocationTransform, allocation)` @ v2.5.2.
#[test]
fn allocation() {
    let mut al = AllocationTransform::new();

    al.set_allocation(Allocation::Uniform);
    assert!(al.validate().is_ok());

    let mut envs: Vec<f32> = vec![0.0f32; 2];
    al.set_vars(&envs);
    assert!(al.validate().is_ok());

    envs.push(0.01f32);
    al.set_vars(&envs);
    assert!(al.validate().is_err());

    al.set_allocation(Allocation::Lg2);
    assert!(al.validate().is_ok());

    envs.push(0.1f32);
    al.set_vars(&envs);
    assert!(al.validate().is_err());

    al.set_vars(&[]);
    assert!(al.validate().is_ok());
}
