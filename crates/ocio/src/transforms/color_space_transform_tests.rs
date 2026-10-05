// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the color space transform: `tests/cpu/transforms/ColorSpaceTransform_tests.cpp` @
//! v2.5.2. Its other tests build ops (WP 3.2). The text and the validation are compared with
//! the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(ColorSpaceTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut cst = ColorSpaceTransform::new();
    assert_eq!(cst.direction(), TransformDirection::Forward);
    cst.set_direction(TransformDirection::Inverse);
    assert_eq!(cst.direction(), TransformDirection::Inverse);

    let empty: &[u8] = b"";

    assert_eq!(empty, cst.src());
    let src: &[u8] = b"source";
    cst.set_src(src);
    assert_eq!(src, cst.src());

    assert_eq!(empty, cst.dst());
    let dst: &[u8] = b"destination";
    cst.set_dst(dst);
    assert_eq!(dst, cst.dst());

    assert!(cst.data_bypass());
    cst.set_data_bypass(false);
    assert!(!cst.data_bypass());
    cst.set_data_bypass(true);
    assert!(cst.data_bypass());

    assert!(cst.validate().is_ok());

    cst.set_src("");
    check_throw_what(
        cst.validate(),
        "ColorSpaceTransform: empty source color space name",
    );
    cst.set_src(src);

    cst.set_dst("");
    check_throw_what(
        cst.validate(),
        "ColorSpaceTransform: empty destination color space name",
    );
    cst.set_dst(dst);
}

/// The setters keep a name up to its first NUL, as from a C string, and a copy keeps every
/// field (upstream's `createEditableCopy`, `Impl::operator=`, ColorSpaceTransform.cpp:41-50,
/// 61-66).
#[test]
fn names_stop_at_nul_and_copies_keep_every_field() {
    let mut cst = ColorSpaceTransform::new();
    cst.set_src(b"ab\0cd");
    cst.set_dst(b"\0x");
    assert_eq!(cst.src(), b"ab");
    assert_eq!(cst.dst(), b"");
    check_throw_what(
        cst.validate(),
        "ColorSpaceTransform: empty destination color space name",
    );

    cst.set_dst("d");
    cst.set_direction(TransformDirection::Inverse);
    cst.set_data_bypass(false);
    let copy = cst.clone();
    assert_eq!(copy.src(), b"ab");
    assert_eq!(copy.dst(), b"d");
    assert_eq!(copy.direction(), TransformDirection::Inverse);
    assert!(!copy.data_bypass());
}
