// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingtone/GradingTone_tests.cpp @ v2.5.2.

use super::*;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(GradingTone, basic)` @ v2.5.2.
#[test]
fn basic() {
    let rgbm0 = GradingRgbmsw::default();
    assert_eq!(rgbm0.red, 1.);
    assert_eq!(rgbm0.green, 1.);
    assert_eq!(rgbm0.blue, 1.);
    assert_eq!(rgbm0.master, 1.);
    assert_eq!(rgbm0.start, 0.);
    assert_eq!(rgbm0.width, 1.);

    let rgbm1 = GradingRgbmsw::new(1., 2., 3., 4., 5., 6.);
    assert_eq!(rgbm1.red, 1.);
    assert_eq!(rgbm1.green, 2.);
    assert_eq!(rgbm1.blue, 3.);
    assert_eq!(rgbm1.master, 4.);
    assert_eq!(rgbm1.start, 5.);
    assert_eq!(rgbm1.width, 6.);

    let mut rgbm2 = rgbm1;
    assert_eq!(rgbm2.red, 1.);
    assert_eq!(rgbm2.green, 2.);
    assert_eq!(rgbm2.blue, 3.);
    assert_eq!(rgbm2.master, 4.);
    assert_eq!(rgbm2.start, 5.);
    assert_eq!(rgbm2.width, 6.);

    // Check operator==().
    assert_eq!(rgbm1, rgbm2);
    rgbm2.red += 0.1111;
    assert_ne!(rgbm1, rgbm2);

    // Check defaults.
    let tone_log = GradingTone::new(GradingStyle::Log);
    assert_eq!(
        tone_log.blacks,
        GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.4)
    );
    assert_eq!(
        tone_log.shadows,
        GradingRgbmsw::new(1., 1., 1., 1., 0.5, 0.)
    );
    assert_eq!(
        tone_log.midtones,
        GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.6)
    );
    assert_eq!(
        tone_log.highlights,
        GradingRgbmsw::new(1., 1., 1., 1., 0.3, 1.)
    );
    assert_eq!(
        tone_log.whites,
        GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.5)
    );
    assert_eq!(tone_log.scontrast, 1.);
    let tone_lin = GradingTone::new(GradingStyle::Lin);
    assert_eq!(tone_lin.blacks, GradingRgbmsw::new(1., 1., 1., 1., 0., 4.));
    assert_eq!(
        tone_lin.shadows,
        GradingRgbmsw::new(1., 1., 1., 1., 2., -7.)
    );
    assert_eq!(
        tone_lin.midtones,
        GradingRgbmsw::new(1., 1., 1., 1., 0., 8.)
    );
    assert_eq!(
        tone_lin.highlights,
        GradingRgbmsw::new(1., 1., 1., 1., -2., 9.)
    );
    assert_eq!(tone_lin.whites, GradingRgbmsw::new(1., 1., 1., 1., 0., 8.));
    assert_eq!(tone_lin.scontrast, 1.);
    let tone_vid = GradingTone::new(GradingStyle::Video);
    assert_eq!(
        tone_vid.blacks,
        GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.4)
    );
    assert_eq!(
        tone_vid.shadows,
        GradingRgbmsw::new(1., 1., 1., 1., 0.6, 0.)
    );
    assert_eq!(
        tone_vid.midtones,
        GradingRgbmsw::new(1., 1., 1., 1., 0.4, 0.7)
    );
    assert_eq!(
        tone_vid.highlights,
        GradingRgbmsw::new(1., 1., 1., 1., 0.2, 1.)
    );
    assert_eq!(
        tone_vid.whites,
        GradingRgbmsw::new(1., 1., 1., 1., 0.5, 0.5)
    );
    assert_eq!(tone_vid.scontrast, 1.);

    let mut gt1 = GradingTone::new(GradingStyle::Log);
    gt1.midtones.start = 0.1;

    let gt2 = gt1;
    // Check operator==().
    assert_eq!(gt1, gt2);
    gt1.highlights.red += 0.1111;
    assert_ne!(gt1, gt2);
}

/// Port of `OCIO_ADD_TEST(GradingTone, rgbmsw_channel)` @ v2.5.2.
#[test]
fn rgbmsw_channel() {
    let rgbm1 = GradingRgbmsw::new(1., 2., 3., 4., 5., 6.);
    assert_eq!(get_channel_value(&rgbm1, RgbmChannel::R), 1.);
    assert_eq!(get_channel_value(&rgbm1, RgbmChannel::G), 2.);
    assert_eq!(get_channel_value(&rgbm1, RgbmChannel::B), 3.);
    assert_eq!(get_channel_value(&rgbm1, RgbmChannel::M), 4.);
}

/// Port of `OCIO_ADD_TEST(GradingTone, validate)` @ v2.5.2.
#[test]
fn validate() {
    let mut tone = GradingTone::new(GradingStyle::Log);
    assert!(tone.validate().is_ok());

    let mut temp = tone.blacks.red;
    tone.blacks.red = 0.08;
    check_throw_what(tone.validate(), "are below lower bound");
    tone.blacks.red = temp;

    temp = tone.midtones.width;
    tone.midtones.width = 0.001;
    check_throw_what(tone.validate(), "is below lower bound");
    tone.midtones.width = temp;

    temp = tone.whites.blue;
    tone.whites.blue = 2.;
    check_throw_what(tone.validate(), "are above upper bound");
    tone.whites.blue = temp;

    temp = tone.shadows.master;
    tone.shadows.master = 0.15;
    check_throw_what(tone.validate(), "are below lower bound");
    tone.shadows.master = temp;

    temp = tone.highlights.green;
    tone.highlights.green = 1.9;
    check_throw_what(tone.validate(), "are above upper bound");
    tone.highlights.green = temp;

    temp = tone.scontrast;
    tone.scontrast = 2.;
    check_throw_what(tone.validate(), "is above upper bound");
    tone.scontrast = temp;
    assert!(tone.validate().is_ok());
}
