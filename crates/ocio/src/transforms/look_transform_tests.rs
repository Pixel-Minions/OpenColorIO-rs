// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the look transform: `tests/cpu/transforms/LookTransform_tests.cpp` @ v2.5.2. Its
//! other tests build ops (WP 3.2, and fixed functions in `p3-after-p2`). The text and the
//! validation are compared with the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(LookTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut look = LookTransform::new();
    assert_eq!(look.direction(), TransformDirection::Forward);

    look.set_direction(TransformDirection::Inverse);
    assert_eq!(look.direction(), TransformDirection::Inverse);

    assert_eq!(look.src(), b"");
    assert_eq!(look.dst(), b"");
    assert_eq!(look.looks(), b"");

    check_throw_what(look.validate(), "empty source");

    let src: &[u8] = b"src";
    let dst: &[u8] = b"dst";
    let looks: &[u8] = b"look1, look2, look3";

    look.set_src(src);
    assert_eq!(look.src(), src);

    check_throw_what(look.validate(), "empty destination");

    look.set_dst(dst);
    assert_eq!(look.dst(), dst);

    assert!(look.validate().is_ok());

    look.set_looks(looks);
    assert_eq!(look.looks(), looks);

    assert!(!look.skip_color_space_conversion());
    look.set_skip_color_space_conversion(true);
    assert!(look.skip_color_space_conversion());

    // Copy and check copy has same values.
    let tr: crate::Transform = look.clone().into();
    let crate::Transform::Look(mut look) = tr else {
        panic!("a look transform");
    };
    assert_eq!(look.src(), src);
    assert_eq!(look.dst(), dst);
    assert_eq!(look.looks(), looks);
    assert!(look.skip_color_space_conversion());

    // Using null is similar as using an empty string. (A null pointer is an empty slice here.)
    look.set_src(b"");
    assert_eq!(look.src(), b"");

    look.set_dst(b"");
    assert_eq!(look.dst(), b"");
}

/// The setters keep a name up to its first NUL, as from a C string.
#[test]
fn names_stop_at_nul() {
    let mut look = LookTransform::new();
    look.set_src(b"a\0b");
    look.set_dst(b"\0");
    look.set_looks(b"+l1,\0-l2");
    assert_eq!(look.src(), b"a");
    assert_eq!(look.dst(), b"");
    assert_eq!(look.looks(), b"+l1,");
}
