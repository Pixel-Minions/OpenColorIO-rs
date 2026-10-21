// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingprimary/GradingPrimaryOpData_tests.cpp @ v2.5.2.

use super::*;
use crate::open_color_types::DynamicPropertyType;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    // Create GradingPrimaryOpData and check values. More tests are done using
    // GradingPrimaryTransform.
    let mut gp = GradingPrimaryOpData::new(GradingStyle::Lin);

    assert_eq!(gp.get_style(), GradingStyle::Lin);
    let gdp_lin = GradingPrimary::new(GradingStyle::Lin);
    assert_eq!(gp.get_value(), gdp_lin);
    assert_eq!(gp.get_direction(), TransformDirection::Forward);

    gp.set_style(GradingStyle::Log);
    assert_eq!(gp.get_style(), GradingStyle::Log);
    let gdp_log = GradingPrimary::new(GradingStyle::Log);
    gp.set_value(&gdp_log).unwrap();
    assert_eq!(gp.get_value(), gdp_log);
    gp.set_direction(TransformDirection::Inverse);
    assert_eq!(gp.get_direction(), TransformDirection::Inverse);

    assert_eq!(gp.get_type(), OpDataType::GradingPrimary);
    assert!(gp.is_no_op());
    assert!(gp.is_identity());
    assert!(!gp.has_channel_crosstalk());

    let expected = "log inverse <brightness=<r=0, g=0, b=0, m=0>, \
                    contrast=<r=1, g=1, b=1, m=1>, gamma=<r=1, g=1, b=1, m=1>, \
                    offset=<r=0, g=0, b=0, m=0>, exposure=<r=0, g=0, b=0, m=0>, \
                    lift=<r=0, g=0, b=0, m=0>, gain=<r=1, g=1, b=1, m=1>, \
                    saturation=1, pivot=<contrast=-0.2, black=0, white=1>>";
    assert_eq!(gp.get_cache_id(), expected.as_bytes());

    // Test operator==.
    let mut gp1 = GradingPrimaryOpData::new(GradingStyle::Lin);
    let mut gp2 = GradingPrimaryOpData::new(GradingStyle::Lin);

    assert!(gp1 == gp2);
    gp1.set_direction(TransformDirection::Inverse);
    assert!(!(gp1 == gp2));
    gp2.set_direction(TransformDirection::Inverse);
    assert!(gp1 == gp2);

    gp1.set_style(GradingStyle::Log);
    assert!(!(gp1 == gp2));
    gp2.set_style(GradingStyle::Log);
    assert!(gp1 == gp2);

    let mut v1 = gp1.get_value();
    v1.brightness.red += 0.1;
    gp1.set_value(&v1).unwrap();
    assert!(!(gp1 == gp2));
    let mut v2 = gp2.get_value();
    v2.brightness.red += 0.1;
    gp2.set_value(&v2).unwrap();
    assert!(gp1 == gp2);

    v1.pivot_black += 0.1;
    gp1.set_value(&v1).unwrap();
    assert!(!(gp1 == gp2));
    v2.pivot_black += 0.1;
    gp2.set_value(&v2).unwrap();
    assert!(gp1 == gp2);

    // IsIdentity.

    assert!(!gp1.is_identity());

    let gp3 = GradingPrimaryOpData::new(GradingStyle::Lin);
    assert!(gp3.is_identity());

    let mut v3 = gp3.get_value();
    v3.clamp_black = 0.5;
    gp3.set_value(&v3).unwrap();
    assert!(!gp3.is_identity());

    // Channel crosstalk.

    assert!(!gp3.has_channel_crosstalk());
    v3.saturation = 0.5;
    gp3.set_value(&v3).unwrap();
    assert!(gp3.has_channel_crosstalk());

    // Check isInverse.

    // We have equal ops, inverse one.
    gp1.set_direction(TransformDirection::Forward);
    // Need a shared pointer for the parameter.
    let gpptr2 = gp1.clone();
    gp1.set_direction(TransformDirection::Inverse);
    assert!(gp1.is_inverse(&gpptr2));
    // Change value of one: no longer inverse.
    v1.pivot_black += 0.1;
    gp1.set_value(&v1).unwrap();
    assert!(!gp1.is_inverse(&gpptr2));
    // Restore value.
    v1.pivot_black -= 0.1;
    gp1.set_value(&v1).unwrap();
    assert!(gp1.is_inverse(&gpptr2));
    // Change direction: no longer inverse.
    gp1.set_direction(TransformDirection::Forward);
    assert!(!gp1.is_inverse(&gpptr2));
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpData, validate)` @ v2.5.2.
#[test]
fn validate() {
    let gp = GradingPrimaryOpData::new(GradingStyle::Log);
    gp.validate().unwrap();

    let mut v = gp.get_value();

    // Test invalid gamma.
    v.gamma.red = 0.0001;
    check_throw_what(
        gp.set_value(&v),
        "GradingPrimary gamma '<r=0.0001, g=1, b=1, m=1>' are below lower bound (0.01)",
    );

    v.gamma.red = 1.;
    v.gamma.green = 0.0001;
    check_throw_what(gp.set_value(&v), "are below lower bound (0.01)");

    v.gamma.green = 1.;
    v.gamma.blue = 0.0001;
    check_throw_what(gp.set_value(&v), "are below lower bound (0.01)");

    v.gamma.blue = 1.;
    v.gamma.master = 0.0001;
    check_throw_what(gp.set_value(&v), "are below lower bound (0.01)");

    v.gamma.master = 1.;
    gp.set_value(&v).unwrap();
    gp.validate().unwrap();

    // Test invalid pivot.
    v.pivot_black = 0.5;
    v.pivot_white = 0.4;
    check_throw_what(
        gp.set_value(&v),
        "black pivot should be smaller than white pivot",
    );

    v.pivot_black = 0.;
    gp.set_value(&v).unwrap();
    gp.validate().unwrap();

    // Test invalid clamp.
    v.clamp_black = 0.5;
    v.clamp_white = 0.4;
    check_throw_what(
        gp.set_value(&v),
        "black clamp should be smaller than white clamp",
    );

    v.clamp_black = 0.;
    gp.set_value(&v).unwrap();
    gp.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpData, dynamic)` @ v2.5.2.
#[test]
fn dynamic() {
    // Create GradingPrimaryOpData and check values. More tests are done using
    // GradingPrimaryTransform.
    let gp = GradingPrimaryOpData::new(GradingStyle::Lin);

    assert!(!gp.is_dynamic());
    let dp = gp.get_dynamic_property();

    let dgp = gp.get_dynamic_property_internal();
    assert!(!dgp.is_dynamic());
    dgp.make_dynamic();

    assert!(gp.is_dynamic());
    assert_eq!(dp.get_type(), DynamicPropertyType::GradingPrimary);

    let mut gdp = GradingPrimary::new(GradingStyle::Lin);
    gdp.pivot_black = 0.01;
    dgp.set_value(&gdp).unwrap();

    assert_eq!(gp.get_value().pivot_black, 0.01);

    dgp.make_non_dynamic();

    assert!(!gp.is_dynamic());
}
