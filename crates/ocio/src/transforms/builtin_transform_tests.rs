// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the built-in transform: `tests/cpu/transforms/BuiltinTransform_tests.cpp` @ v2.5.2.
//! `color_matrix_helpers` comes with the helpers (WP 3.2e); the tests that build ops with
//! the builders and Phase 2 (`p3-after-p2`). The text and the validation are compared with the
//! wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(BuiltinTransform, creation)` @ v2.5.2.
#[test]
fn creation() {
    // Tests around the creation of a built-in transform instance.

    let mut blt = BuiltinTransform::new();

    assert_eq!(blt.direction(), TransformDirection::Forward);
    assert_eq!(blt.style(), b"IDENTITY");
    assert!(blt.validate().is_ok());

    assert!(
        blt.set_style("UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD")
            .is_ok()
    );
    assert_eq!(blt.style(), b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
    assert!(blt.validate().is_ok());

    assert_eq!(
        &b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"[..],
        blt.description()
    );

    blt.set_direction(TransformDirection::Inverse);
    assert_eq!(blt.direction(), TransformDirection::Inverse);
    assert!(blt.validate().is_ok());

    // The style is case insensitive.
    assert!(
        blt.set_style("UTILITY - ACES-AP0_to_cie-xyz-D65_BFD")
            .is_ok()
    );
    assert!(blt.validate().is_ok());

    // Try an unknown style.
    check_throw_what(
        blt.set_style("UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD_UNKNOWN"),
        "BuiltinTransform: invalid built-in transform style \
         'UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD_UNKNOWN'.",
    );
}

/// Port of `OCIO_ADD_TEST(BuiltinTransform, access)` @ v2.5.2.
#[test]
fn access() {
    // Only test some default built-in transforms.

    assert_eq!(
        &b"IDENTITY"[..],
        BuiltinTransformRegistry::get().builtin_style(0).unwrap()
    );

    assert_eq!(
        &b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD"[..],
        BuiltinTransformRegistry::get().builtin_style(1).unwrap()
    );

    assert_eq!(
        &b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"[..],
        BuiltinTransformRegistry::get()
            .builtin_description(1)
            .unwrap()
    );
}

/// A style is read up to its first NUL; an unknown one leaves the transform as it was.
#[test]
fn set_style_stops_at_nul_and_keeps_the_style_on_error() {
    let mut blt = BuiltinTransform::new();
    blt.set_style(b"utility - aces-ap0_to_cie-xyz-d65_bfd\0junk")
        .unwrap();
    assert_eq!(blt.style(), b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
    check_throw_what(
        blt.set_style(b"\0IDENTITY"),
        "BuiltinTransform: invalid built-in transform style ''.",
    );
    assert_eq!(blt.style(), b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
}
