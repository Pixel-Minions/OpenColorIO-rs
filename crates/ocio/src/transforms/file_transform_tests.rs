// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the file transform: `tests/cpu/transforms/FileTransform_tests.cpp` @ v2.5.2. Its
//! other tests read files (Phase 4) or resolve context variables (WP 3.2a). The text and the
//! validation are compared with the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(FileTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut ft = FileTransform::new();
    assert_eq!(ft.direction(), TransformDirection::Forward);
    ft.set_direction(TransformDirection::Inverse);
    assert_eq!(ft.direction(), TransformDirection::Inverse);

    assert_eq!(ft.src(), b"");
    let src: &[u8] = b"source";
    ft.set_src(src);
    assert_eq!(src, ft.src());

    assert_eq!(ft.ccc_id(), b"");
    let cccid: &[u8] = b"cccid";
    ft.set_ccc_id(cccid);
    assert_eq!(cccid, ft.ccc_id());

    assert_eq!(ft.cdl_style(), CdlStyle::NoClamp);
    ft.set_cdl_style(CdlStyle::Asc);
    assert_eq!(ft.cdl_style(), CdlStyle::Asc);

    assert_eq!(ft.interpolation(), Interpolation::Default);
    ft.set_interpolation(Interpolation::Linear);
    assert_eq!(ft.interpolation(), Interpolation::Linear);
}

/// Port of `OCIO_ADD_TEST(FileTransform, validate)` @ v2.5.2.
#[test]
fn validate() {
    let mut tr = FileTransform::new();

    tr.set_src("lut3d_17x17x17_32f_12i.clf");
    assert!(tr.validate().is_ok());

    tr.set_src("");
    check_throw_what(tr.validate(), "FileTransform: empty file path");
}

/// The setters keep a string up to its first NUL, as from a C string, and a copy keeps every
/// field (`Impl::operator=`, FileTransform.cpp:51, 61-66).
#[test]
fn strings_stop_at_nul_and_copies_keep_every_field() {
    let mut ft = FileTransform::new();
    ft.set_src(b"lut.cube\0.3dl");
    ft.set_ccc_id(b"\0id");
    assert_eq!(ft.src(), b"lut.cube");
    assert_eq!(ft.ccc_id(), b"");

    ft.set_ccc_id("shot_010");
    ft.set_cdl_style(CdlStyle::Asc);
    ft.set_interpolation(Interpolation::Unknown);
    ft.set_direction(TransformDirection::Inverse);
    let copy = ft.clone();
    assert_eq!(copy.src(), b"lut.cube");
    assert_eq!(copy.ccc_id(), b"shot_010");
    assert_eq!(copy.cdl_style(), CdlStyle::Asc);
    assert_eq!(copy.interpolation(), Interpolation::Unknown);
    assert_eq!(copy.direction(), TransformDirection::Inverse);
    // An unknown interpolation is valid (version 1 configs use it).
    assert!(copy.validate().is_ok());
}
