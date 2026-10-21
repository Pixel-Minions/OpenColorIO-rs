// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingprimary/GradingPrimary_tests.cpp @ v2.5.2.

use super::*;
use ocio_testkit::upstream::{check_close, check_throw_what};

/// Port of `OCIO_ADD_TEST(GradingPrimary, basic)` @ v2.5.2.
#[test]
fn basic() {
    let rgbm0 = GradingRgbm::default();
    assert_eq!(rgbm0.red, 0.);
    assert_eq!(rgbm0.green, 0.);
    assert_eq!(rgbm0.blue, 0.);
    assert_eq!(rgbm0.master, 0.);

    let rgbm1 = GradingRgbm::new(1., 2., 3., 4.);
    assert_eq!(rgbm1.red, 1.);
    assert_eq!(rgbm1.green, 2.);
    assert_eq!(rgbm1.blue, 3.);
    assert_eq!(rgbm1.master, 4.);

    let mut rgbm2 = rgbm1;
    assert_eq!(rgbm2.red, 1.);
    assert_eq!(rgbm2.green, 2.);
    assert_eq!(rgbm2.blue, 3.);
    assert_eq!(rgbm2.master, 4.);

    // Check operator==().
    assert_eq!(rgbm1, rgbm2);
    rgbm2.red += 0.1111;
    assert_ne!(rgbm1, rgbm2);

    let gp_log = GradingPrimary::new(GradingStyle::Log);
    let gp_lin = GradingPrimary::new(GradingStyle::Lin);
    let gp_vid = GradingPrimary::new(GradingStyle::Video);

    // Check operator==(). (The default values for linear and video are the same. The style is
    // not part of the GradingPrimary struct.)
    assert_eq!(gp_lin, gp_vid);
    assert_ne!(gp_log, gp_lin);
}

/// Port of `OCIO_ADD_TEST(GradingPrimary, validate)` @ v2.5.2.
#[test]
fn validate() {
    let mut gp = GradingPrimary::new(GradingStyle::Log);
    assert!(gp.validate(GradingStyle::Log).is_ok());

    // LOG & VIDEO have to keep gamma above a threshold.
    gp.gamma.red = 0.0001;
    check_throw_what(
        gp.validate(GradingStyle::Log),
        "GradingPrimary gamma '<r=0.0001, g=1, b=1, m=1>' are below lower bound (0.01)",
    );
    check_throw_what(
        gp.validate(GradingStyle::Video),
        "GradingPrimary gamma '<r=0.0001, g=1, b=1, m=1>' are below lower bound (0.01)",
    );

    // LIN does not use gamma.
    assert!(gp.validate(GradingStyle::Lin).is_ok());

    // Restore gamma.
    gp.gamma.red = 1.;

    // LIN has to keep contrast above a threshold.
    gp.contrast.green = 0.0001;
    check_throw_what(
        gp.validate(GradingStyle::Lin),
        "GradingPrimary contrast '<r=1, g=0.0001, b=1, m=1>' are below lower bound (0.01)",
    );

    // LOG can use any contrast value and VIDEO does not use contrast.
    assert!(gp.validate(GradingStyle::Log).is_ok());
    assert!(gp.validate(GradingStyle::Video).is_ok());

    // Upstream then restores contrast (`gp.m_contrast.m_green = 1.`), which nothing reads.
}

/// Port of `OCIO_ADD_TEST(GradingPrimary, precompute)` @ v2.5.2.
#[test]
fn precompute() {
    let fwd = TransformDirection::Forward;
    let mut gp = GradingPrimary::new(GradingStyle::Log);

    let mut comp = GradingPrimaryPreRender::default();
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(*comp.get_brightness() == [0.0f32, 0.0, 0.0]);
    assert!(*comp.get_contrast() == [1.0f32, 1.0, 1.0]);
    assert!(*comp.get_gamma() == [1.0f32, 1.0, 1.0]);
    check_close(comp.get_pivot(), f64::from(0.4f32), f64::from(1.0e-6f32));
    assert!(comp.get_local_bypass());
    assert!(comp.is_gamma_identity());

    gp.saturation = 0.5;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(!comp.get_local_bypass());
    gp.saturation = 1.;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(comp.get_local_bypass());

    gp.brightness.green = 0.1 * 1023. / 6.25;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(*comp.get_brightness() == [0.0f32, 0.1, 0.0]);
    assert!(!comp.get_local_bypass());
    assert!(comp.is_gamma_identity());

    gp.brightness.red = 0.1 * 1023. / 6.25;
    gp.brightness.green = 0.;
    gp.contrast.red = 0.; // Inverse will be 1.
    gp.contrast.green = 1.25;
    gp.gamma.blue = 0.8;
    gp.pivot = 1.;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(*comp.get_brightness() == [0.1f32, 0.0, 0.0]);
    assert!(*comp.get_contrast() == [0.0f32, 1.25, 1.0]);
    assert!(*comp.get_gamma() == [1.0f32, 1.0, 1.25]);
    check_close(comp.get_pivot(), f64::from(1.0f32), f64::from(1.0e-6f32));
    assert!(!comp.is_gamma_identity());

    comp.update(GradingStyle::Log, TransformDirection::Inverse, &gp);
    assert!(*comp.get_brightness() == [-0.1f32, 0.0, 0.0]);
    assert!(*comp.get_contrast() == [1.0f32, 0.8, 1.0]);
    assert!(*comp.get_gamma() == [1.0f32, 1.0, 0.8]);

    gp = GradingPrimary::new(GradingStyle::Log);

    // Test identity checks for GRADING_LOG
    gp.gamma.red = 0.8;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(!comp.is_gamma_identity());
    assert!(!comp.get_local_bypass());
    gp.gamma.red = 1.0;
    comp.update(GradingStyle::Log, fwd, &gp);
    assert!(comp.is_gamma_identity());
    assert!(comp.get_local_bypass());

    // Test identity checks for GRADING_LIN
    gp.contrast.red = 0.8;
    comp.update(GradingStyle::Lin, fwd, &gp);
    assert!(!comp.is_contrast_identity());
    assert!(!comp.get_local_bypass());
    gp.contrast.red = 1.0;
    comp.update(GradingStyle::Lin, fwd, &gp);
    assert!(comp.is_contrast_identity());
    assert!(comp.get_local_bypass());

    // Test identity checks for GRADING_VIDEO
    gp.gamma.red = 0.8;
    comp.update(GradingStyle::Video, fwd, &gp);
    assert!(!comp.is_gamma_identity());
    assert!(!comp.get_local_bypass());
    gp.gamma.red = 1.0;
    comp.update(GradingStyle::Video, fwd, &gp);
    assert!(comp.is_gamma_identity());
    assert!(comp.get_local_bypass());
}

/// The negations of the inverse directions flip a NaN's sign, as the wheels' `xorps` does (on
/// Windows the log brightness's negation is folded into `-6.25`, which keeps it): the same
/// value through the forward and the inverse direction differs only in the sign bit. Run in
/// debug and release builds, where an optimizer could fold the negation.
#[test]
fn inverse_negations_flip_the_sign_of_a_nan() {
    let nan = f64::from_bits(0x7ff8_0000_1234_0000);
    let mut v = GradingPrimary::new(GradingStyle::Lin);
    v.offset.red = nan;
    let mut fwd = GradingPrimaryPreRender::default();
    let mut inv = GradingPrimaryPreRender::default();
    fwd.update(GradingStyle::Lin, TransformDirection::Forward, &v);
    inv.update(GradingStyle::Lin, TransformDirection::Inverse, &v);
    assert!(fwd.get_offset()[0].is_nan());
    assert_eq!(
        inv.get_offset()[0].to_bits(),
        fwd.get_offset()[0].to_bits() ^ 0x8000_0000
    );

    let mut v = GradingPrimary::new(GradingStyle::Video);
    v.offset.red = nan;
    fwd.update(GradingStyle::Video, TransformDirection::Forward, &v);
    inv.update(GradingStyle::Video, TransformDirection::Inverse, &v);
    assert_eq!(
        inv.get_offset()[0].to_bits(),
        fwd.get_offset()[0].to_bits() ^ 0x8000_0000
    );

    let mut v = GradingPrimary::new(GradingStyle::Log);
    v.brightness.red = nan;
    fwd.update(GradingStyle::Log, TransformDirection::Forward, &v);
    inv.update(GradingStyle::Log, TransformDirection::Inverse, &v);
    let flip = if cfg!(windows) { 0 } else { 0x8000_0000 };
    assert_eq!(
        inv.get_brightness()[0].to_bits(),
        fwd.get_brightness()[0].to_bits() ^ flip
    );
}
