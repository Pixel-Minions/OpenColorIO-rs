// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the double-property tests of `tests/cpu/DynamicProperty_tests.cpp` @ v2.5.2. The
//! others need the grading properties (Phase 5) or a processor that reads CTF files.
//!
//! Upstream's `OCIO_REQUIRE_ASSERT(dp)` checks that `std::make_shared` gave a pointer; an
//! `Arc` always holds one.

use super::*;
use ocio_testkit::upstream::check_equal;

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, basic)` @ v2.5.2.
#[test]
fn basic() {
    let dp: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    check_equal(dp.get_value(), 1.0);
    dp.set_value(2.0);
    check_equal(dp.get_value(), 2.0);

    let dp_impl: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    assert!(!dp_impl.is_dynamic());
    check_equal(dp_impl.get_value(), 1.0);

    dp_impl.make_dynamic();
    assert!(dp_impl.is_dynamic());
    dp_impl.set_value(2.0);
    check_equal(dp_impl.get_value(), 2.0);
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, equal_double)` @ v2.5.2.
#[test]
fn equal_double() {
    let dp_impl0: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    let dp0 = DynamicPropertyRcPtr::from(dp_impl0.clone());

    let dp_impl1: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    let dp1 = DynamicPropertyRcPtr::from(dp_impl1.clone());

    // Both not dynamic, same value.
    assert!(dp0 == dp1);

    // Both not dynamic, diff values.
    dp_impl0.set_value(2.0);
    assert!(!(dp0 == dp1));

    // Same value.
    dp_impl1.set_value(2.0);
    assert!(dp0 == dp1);

    // One dynamic, not the other, same value.
    dp_impl0.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, same value. Equality is used to optimized, so if values are dynamic they
    // might or not be the same, but they are considered different so that they are not
    // optimized.
    dp_impl1.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, different values.
    dp_impl1.set_value(3.0);
    assert!(!(dp0 == dp1));
}
