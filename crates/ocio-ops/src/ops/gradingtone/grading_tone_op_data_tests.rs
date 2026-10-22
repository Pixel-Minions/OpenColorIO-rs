// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingtone/GradingToneOpData_tests.cpp @ v2.5.2.

use super::*;
use crate::open_color_types::DynamicPropertyType;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(GradingToneOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    // Create GradingToneOpData and check values. More tests are done using
    // GradingToneTransform.
    let mut g_tone_op_data = GradingToneOpData::new(GradingStyle::Lin);

    assert_eq!(g_tone_op_data.get_style(), GradingStyle::Lin);
    let mut g_tone_val = GradingTone::new(GradingStyle::Lin);
    assert_eq!(g_tone_op_data.get_value(), g_tone_val);
    assert_eq!(g_tone_op_data.get_direction(), TransformDirection::Forward);

    g_tone_op_data.set_style(GradingStyle::Log);
    assert_eq!(g_tone_op_data.get_style(), GradingStyle::Log);
    // Setting the style reset values to default.
    let def_log = GradingTone::new(GradingStyle::Log);
    assert_eq!(g_tone_op_data.get_value(), def_log);
    g_tone_val.scontrast += 0.1;
    g_tone_op_data.set_value(&g_tone_val).unwrap();
    assert_eq!(g_tone_op_data.get_value(), g_tone_val);

    // ... except if it is the current style.
    g_tone_op_data.set_style(GradingStyle::Log);
    assert_eq!(g_tone_op_data.get_value(), g_tone_val);

    g_tone_op_data.set_direction(TransformDirection::Inverse);
    assert_eq!(g_tone_op_data.get_direction(), TransformDirection::Inverse);

    assert_eq!(g_tone_op_data.get_type(), OpDataType::GradingTone);
    g_tone_val.scontrast -= 0.1;
    g_tone_op_data.set_value(&g_tone_val).unwrap();
    assert!(g_tone_op_data.is_no_op());
    assert!(g_tone_op_data.is_identity());
    assert!(!g_tone_op_data.has_channel_crosstalk());

    let expected = "log inverse \
                    <blacks=<red=1 green=1 blue=1 master=1 start=0 width=4> \
                    shadows=<red=1 green=1 blue=1 master=1 start=2 width=-7> \
                    midtones=<red=1 green=1 blue=1 master=1 start=0 width=8> \
                    highlights=<red=1 green=1 blue=1 master=1 start=-2 width=9> \
                    whites=<red=1 green=1 blue=1 master=1 start=0 width=8> \
                    s_contrast=1>";
    assert_eq!(
        String::from_utf8_lossy(&g_tone_op_data.get_cache_id()),
        expected
    );

    // Test operator==.
    let mut gt1 = GradingToneOpData::new(GradingStyle::Lin);
    let mut gt2 = GradingToneOpData::new(GradingStyle::Lin);

    assert!(gt1 == gt2);
    gt1.set_direction(TransformDirection::Inverse);
    assert!(!(gt1 == gt2));
    gt2.set_direction(TransformDirection::Inverse);
    assert!(gt1 == gt2);

    gt1.set_style(GradingStyle::Log);
    assert!(!(gt1 == gt2));
    gt2.set_style(GradingStyle::Log);
    assert!(gt1 == gt2);

    let mut v1 = gt1.get_value();
    v1.midtones.red += 0.1;
    gt1.set_value(&v1).unwrap();
    assert!(!(gt1 == gt2));
    let mut v2 = gt2.get_value();
    v2.midtones.red += 0.1;
    gt2.set_value(&v2).unwrap();
    assert!(gt1 == gt2);

    v1.scontrast += 0.1;
    gt1.set_value(&v1).unwrap();
    assert!(!(gt1 == gt2));
    v2.scontrast += 0.1;
    gt2.set_value(&v2).unwrap();
    assert!(gt1 == gt2);

    assert!(!gt1.is_identity());

    // Check inverse.
    let gt1_inv = gt1.inverse();
    assert!(gt1.is_inverse(&gt1_inv));

    assert_eq!(gt1.get_direction(), TransformDirection::Inverse);
    assert_eq!(gt1_inv.get_direction(), TransformDirection::Forward);
    let v1_inv = gt1_inv.get_value();
    assert_eq!(v1, v1_inv);
}

/// Port of `OCIO_ADD_TEST(GradingToneOpData, validate)` @ v2.5.2.
#[test]
fn validate() {
    let gt = GradingToneOpData::new(GradingStyle::Log);
    gt.validate().unwrap();

    let mut v = gt.get_value();

    // Test invalid black detail.
    v.blacks.red = 2.0;
    check_throw_what(
        gt.set_value(&v),
        "GradingTone blacks '<red=2 green=1 blue=1 master=1 start=0.4 width=0.4>' \
         are above upper bound (1.9)",
    );

    v.blacks.red = 1.5;
    gt.set_value(&v).unwrap();
    gt.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(GradingToneOpData, dynamic)` @ v2.5.2.
#[test]
fn dynamic() {
    // Create GradingToneOpData and check values. More tests are done using
    // GradingToneTransform.
    let gt = GradingToneOpData::new(GradingStyle::Lin);

    assert!(!gt.is_dynamic());
    let dp = gt.get_dynamic_property();

    let dgt = gt.get_dynamic_property_internal();
    assert!(!dgt.is_dynamic());
    dgt.make_dynamic();

    assert!(gt.is_dynamic());
    assert_eq!(dp.get_type(), DynamicPropertyType::GradingTone);

    let mut val = GradingTone::new(GradingStyle::Lin);
    val.scontrast = 1.1;
    dgt.set_value(&val).unwrap();

    assert_eq!(gt.get_value().scontrast, 1.1);

    dgt.make_non_dynamic();

    assert!(!gt.is_dynamic());
}
