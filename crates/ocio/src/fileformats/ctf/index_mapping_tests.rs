// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/fileformats/ctf/IndexMapping_tests.cpp` @ v2.5.2.

use ocio_testkit::upstream::{check_equal, check_throw_what};

use super::*;

/// Port of `OCIO_ADD_TEST(IndexMapping, test_accessors)` @ v2.5.2.
#[test]
fn test_accessors() {
    let mut r#ref = IndexMapping::new(4);
    r#ref.set_pair(0, 0.0, 0.0).unwrap();
    r#ref.set_pair(1, 100.0, 1.0).unwrap();
    r#ref.set_pair(2, 200.0, 2.0).unwrap();
    r#ref.set_pair(3, 300.0, 3.0).unwrap();

    check_throw_what(
        r#ref.set_pair(5, 300.0, 3.0),
        "invalid. Should be less than",
    );

    r#ref.validate().unwrap();

    check_equal(r#ref.get_dimension(), 4);
    let (first, second) = r#ref.get_pair(0).unwrap();
    check_equal(first, 0.0f32);
    check_equal(second, 0.0f32);
    let (first, second) = r#ref.get_pair(1).unwrap();
    check_equal(first, 100.0f32);
    check_equal(second, 1.0f32);
    let (first, second) = r#ref.get_pair(2).unwrap();
    check_equal(first, 200.0f32);
    check_equal(second, 2.0f32);
    let (first, second) = r#ref.get_pair(3).unwrap();
    check_equal(first, 300.0f32);
    check_equal(second, 3.0f32);

    r#ref.resize(8);
    check_equal(r#ref.get_dimension(), 8);
}

/// Port of `OCIO_ADD_TEST(IndexMapping, range_validation)` @ v2.5.2.
#[test]
fn range_validation() {
    let mut r#ref = IndexMapping::new(4);
    r#ref.set_pair(0, 0.0, 0.0).unwrap();
    r#ref.set_pair(1, 100.0, 1.0).unwrap();
    r#ref.set_pair(2, 200.0, 2.0).unwrap();
    r#ref.set_pair(3, 200.0, 3.0).unwrap(); // illegal

    check_throw_what(r#ref.validate(), "Index values must be increasing");

    r#ref.set_pair(3, 300.0, 2.0).unwrap(); // illegal
    check_throw_what(r#ref.validate(), "Index values must be increasing");
}

/// Port of `OCIO_ADD_TEST(IndexMapping, equality)` @ v2.5.2.
#[test]
fn equality() {
    let mut ref1 = IndexMapping::new(4);
    ref1.set_pair(0, 0.0, 0.0).unwrap();
    ref1.set_pair(1, 100.0, 1.0).unwrap();
    ref1.set_pair(2, 200.0, 2.0).unwrap();
    ref1.set_pair(3, 300.0, 3.0).unwrap();

    let mut ref2 = IndexMapping::new(4);
    ref2.set_pair(0, 0.0, 0.0).unwrap();
    ref2.set_pair(1, 100.0, 1.0).unwrap();
    ref2.set_pair(2, 200.0, 2.0).unwrap();
    ref2.set_pair(3, 300.0, 3.0).unwrap();

    assert!(ref1 == ref2);

    ref2.set_pair(2, 200.0, 2.1).unwrap();

    assert!(!(ref1 == ref2));
}
