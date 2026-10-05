// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the display view transform: `tests/cpu/transforms/DisplayViewTransform_tests.cpp`
//! @ v2.5.2. Its other tests build ops (WP 3.2). The text and the validation are compared with
//! the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(DisplayViewTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let empty: &[u8] = b"";
    let mut dt = DisplayViewTransform::new();

    assert_eq!(dt.direction(), TransformDirection::Forward);
    assert_eq!(empty, dt.src());
    assert_eq!(empty, dt.display());
    assert_eq!(empty, dt.view());
    assert!(!dt.looks_bypass());
    assert!(dt.data_bypass());
    dt.set_data_bypass(false);
    assert!(!dt.data_bypass());
    dt.set_data_bypass(true);
    assert!(dt.data_bypass());

    let input_cs: &[u8] = b"inputCS";
    dt.set_src(input_cs);
    assert_eq!(input_cs, dt.src());

    let display: &[u8] = b"display";
    dt.set_display(display);
    assert_eq!(display, dt.display());

    let view: &[u8] = b"view";
    dt.set_view(view);
    assert_eq!(view, dt.view());

    assert!(dt.validate().is_ok());

    dt.set_direction(TransformDirection::Inverse);
    assert_eq!(dt.direction(), TransformDirection::Inverse);

    dt.set_src("");
    check_throw_what(
        dt.validate(),
        "DisplayViewTransform: empty source color space name",
    );
    dt.set_src(input_cs);

    dt.set_display("");
    check_throw_what(dt.validate(), "DisplayViewTransform: empty display name");
    dt.set_display(display);

    dt.set_view("");
    check_throw_what(dt.validate(), "DisplayViewTransform: empty view name");
    dt.set_view(view);

    assert!(dt.validate().is_ok());

    dt.set_looks_bypass(true);
    assert!(dt.looks_bypass());

    dt.set_data_bypass(false);

    // Verify that copy has same values.
    let t: crate::Transform = dt.clone().into();
    let crate::Transform::DisplayView(dt) = t else {
        panic!("a display view transform");
    };
    assert_eq!(input_cs, dt.src());
    assert_eq!(display, dt.display());
    assert_eq!(view, dt.view());
    assert_eq!(dt.direction(), TransformDirection::Inverse);
    assert!(dt.looks_bypass());
    assert!(!dt.data_bypass());
}

/// The setters keep a name up to its first NUL, as from a C string.
#[test]
fn names_stop_at_nul() {
    let mut dt = DisplayViewTransform::new();
    dt.set_src(b"s\0rc");
    dt.set_display(b"d\0");
    dt.set_view(b"\0v");
    assert_eq!(dt.src(), b"s");
    assert_eq!(dt.display(), b"d");
    assert_eq!(dt.view(), b"");
    check_throw_what(dt.validate(), "DisplayViewTransform: empty view name");
}
