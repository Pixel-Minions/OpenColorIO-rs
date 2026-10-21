// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/exposurecontrast/ExposureContrastOpData_tests.cpp @ v2.5.2.

use super::*;

/// The shared double property behind a dynamic property handle.
fn double_of(dp: &DynamicPropertyRcPtr) -> &DynamicPropertyDoubleImplRcPtr {
    let DynamicPropertyRcPtr::Double(dp) = dp else {
        panic!("not a double property");
    };
    dp
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOpData, style)` @ v2.5.2.
#[test]
fn style() {
    let convert_to_style = ExposureContrastOpData::convert_string_to_style;
    let style = convert_to_style(Some(EC_STYLE_LINEAR)).unwrap();
    assert_eq!(style, ExposureContrastOpStyle::Linear);
    let style = convert_to_style(Some(EC_STYLE_LINEAR_REV)).unwrap();
    assert_eq!(style, ExposureContrastOpStyle::LinearRev);
    let style = convert_to_style(Some(EC_STYLE_VIDEO)).unwrap();
    assert_eq!(style, ExposureContrastOpStyle::Video);
    let style = convert_to_style(Some(EC_STYLE_VIDEO_REV)).unwrap();
    assert_eq!(style, ExposureContrastOpStyle::VideoRev);
    let style = convert_to_style(Some(EC_STYLE_LOGARITHMIC)).unwrap();
    assert_eq!(style, ExposureContrastOpStyle::Logarithmic);
    let style = convert_to_style(Some(EC_STYLE_LOGARITHMIC_REV)).unwrap();
    assert_eq!(style, ExposureContrastOpStyle::LogarithmicRev);

    let err = convert_to_style(Some("Unknown exposure contrast style")).unwrap_err();
    assert!(
        err.to_string().contains("Unknown exposure contrast style"),
        "{err}"
    );

    let err = convert_to_style(None).unwrap_err();
    assert!(
        err.to_string().contains("Missing exposure contrast style"),
        "{err}"
    );

    let convert_to_string = ExposureContrastOpData::convert_style_to_string;
    assert_eq!(
        convert_to_string(ExposureContrastOpStyle::Linear),
        EC_STYLE_LINEAR
    );
    assert_eq!(
        convert_to_string(ExposureContrastOpStyle::LinearRev),
        EC_STYLE_LINEAR_REV
    );
    assert_eq!(
        convert_to_string(ExposureContrastOpStyle::Video),
        EC_STYLE_VIDEO
    );
    assert_eq!(
        convert_to_string(ExposureContrastOpStyle::VideoRev),
        EC_STYLE_VIDEO_REV
    );
    assert_eq!(
        convert_to_string(ExposureContrastOpStyle::Logarithmic),
        EC_STYLE_LOGARITHMIC
    );
    assert_eq!(
        convert_to_string(ExposureContrastOpStyle::LogarithmicRev),
        EC_STYLE_LOGARITHMIC_REV
    );
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    let ec0 = ExposureContrastOpData::default();
    assert_eq!(ec0.get_type(), OpDataType::ExposureContrast);
    assert_eq!(ec0.get_style(), ExposureContrastOpStyle::Linear);

    assert_eq!(ec0.get_exposure(), 0.0);
    assert_eq!(ec0.get_contrast(), 1.0);
    assert_eq!(ec0.get_gamma(), 1.0);
    assert_eq!(ec0.get_pivot(), 0.18);
    assert_eq!(
        ec0.get_log_exposure_step(),
        ExposureContrastOpData::LOGEXPOSURESTEP_DEFAULT
    );
    assert_eq!(
        ec0.get_log_mid_gray(),
        ExposureContrastOpData::LOGMIDGRAY_DEFAULT
    );

    assert!(ec0.is_identity());
    assert!(ec0.is_no_op());
    assert!(!ec0.has_channel_crosstalk());
    ec0.validate();

    let cache_id = ec0.get_cache_id();
    let expected = "linear E: 0 C: 1 G: 1 P: 0.18 LES: 0.088 LMG: 0.435";
    assert!(
        strcasecmp(&cache_id, expected).is_eq(),
        "{}",
        String::from_utf8_lossy(&cache_id)
    );

    ec0.set_exposure(0.1);
    assert!(!ec0.is_identity());
    assert!(!ec0.is_no_op());
    assert!(!ec0.has_channel_crosstalk());
    let cache_id_updated = ec0.get_cache_id();
    assert_ne!(cache_id, cache_id_updated);

    let mut ec = ExposureContrastOpData::new(ExposureContrastOpStyle::Video);
    assert_eq!(ec.get_type(), OpDataType::ExposureContrast);
    assert_eq!(ec.get_style(), ExposureContrastOpStyle::Video);

    assert_eq!(ec.get_exposure(), 0.0);
    assert_eq!(ec.get_contrast(), 1.0);
    assert_eq!(ec.get_gamma(), 1.0);
    assert_eq!(ec.get_pivot(), 0.18);
    assert_eq!(
        ec.get_log_exposure_step(),
        ExposureContrastOpData::LOGEXPOSURESTEP_DEFAULT
    );
    assert_eq!(
        ec.get_log_mid_gray(),
        ExposureContrastOpData::LOGMIDGRAY_DEFAULT
    );

    assert!(ec.is_no_op());

    assert!(!ec.get_exposure_property().is_dynamic());
    assert!(!ec.get_contrast_property().is_dynamic());
    assert!(!ec.get_gamma_property().is_dynamic());
    assert!(!ec.is_dynamic());

    // Never treated as NoOp when dynamic.
    ec.get_exposure_property().make_dynamic();
    assert!(!ec.is_no_op());
    assert!(ec.is_dynamic());
    assert!(ec.has_dynamic_property(DynamicPropertyType::Exposure));

    ec.set_exposure(0.1);
    ec.set_contrast(0.8);
    ec.set_gamma(1.1);
    ec.set_pivot(0.2);
    ec.set_log_exposure_step(0.07);
    ec.set_log_mid_gray(0.5);

    assert_eq!(ec.get_exposure(), 0.1);
    assert_eq!(ec.get_contrast(), 0.8);
    assert_eq!(ec.get_gamma(), 1.1);
    assert_eq!(ec.get_pivot(), 0.2);
    assert_eq!(ec.get_log_exposure_step(), 0.07);
    assert_eq!(ec.get_log_mid_gray(), 0.5);

    // Property must be set as dynamic to accept a dynamic value.
    assert!(!ec.has_dynamic_property(DynamicPropertyType::Contrast));
    assert!(!ec.has_dynamic_property(DynamicPropertyType::Gamma));
    let err = ec
        .get_dynamic_property(DynamicPropertyType::Contrast)
        .unwrap_err();
    assert!(err.to_string().contains("not dynamic"), "{err}");

    let dp_exp = ec.get_exposure_property();
    assert_eq!(dp_exp.get_value(), 0.1);
    assert!(!ec.has_dynamic_property(DynamicPropertyType::Contrast));
    assert!(!ec.has_dynamic_property(DynamicPropertyType::Gamma));
    dp_exp.set_value(1.5);
    assert_eq!(ec.get_exposure(), 1.5);
    dp_exp.set_value(0.7);
    assert_eq!(ec.get_exposure(), 0.7);

    ec.get_contrast_property().make_dynamic();
    ec.get_gamma_property().make_dynamic();
    let dp_contrast = ec.get_contrast_property();
    let dp_gamma = ec.get_gamma_property();
    dp_contrast.set_value(1.42);
    dp_gamma.set_value(0.88);
    assert_eq!(ec.get_contrast(), 1.42);
    assert_eq!(ec.get_gamma(), 0.88);
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOpData, clone)` @ v2.5.2.
#[test]
fn clone() {
    let mut ec = ExposureContrastOpData::default();
    ec.set_exposure(-1.4);
    ec.set_contrast(0.8);
    ec.set_gamma(1.1);
    ec.set_pivot(0.2);

    ec.get_exposure_property().make_dynamic();

    let dp_exp = ec.get_exposure_property();
    assert_eq!(dp_exp.get_value(), -1.4);
    dp_exp.set_value(1.5);
    let ec_cloned = ec.clone();

    assert_eq!(ec.get_exposure(), ec_cloned.get_exposure());
    assert_eq!(ec.get_contrast(), ec_cloned.get_contrast());
    assert_eq!(ec.get_gamma(), ec_cloned.get_gamma());
    assert_eq!(ec.get_pivot(), ec_cloned.get_pivot());
    assert_eq!(
        ec.get_log_exposure_step(),
        ec_cloned.get_log_exposure_step()
    );
    assert_eq!(ec.get_log_mid_gray(), ec_cloned.get_log_mid_gray());

    assert_eq!(
        ec.get_exposure_property().is_dynamic(),
        ec_cloned.get_exposure_property().is_dynamic()
    );
    assert_eq!(
        ec.get_contrast_property().is_dynamic(),
        ec_cloned.get_contrast_property().is_dynamic()
    );
    assert_eq!(
        ec.get_gamma_property().is_dynamic(),
        ec_cloned.get_gamma_property().is_dynamic()
    );

    // Clone makes a copy of the dynamic property rather than sharing the original.
    dp_exp.set_value(0.21);
    assert_ne!(ec.get_exposure(), ec_cloned.get_exposure());
    assert_eq!(ec.get_exposure(), 0.21);
    assert_eq!(ec_cloned.get_exposure(), 1.5);
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOpData, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let mut ec = ExposureContrastOpData::new(ExposureContrastOpStyle::Video);

    ec.set_contrast(0.8);
    ec.set_gamma(1.1);
    ec.set_pivot(0.2);

    ec.get_exposure_property().make_dynamic();

    let dp_exp = ec.get_exposure_property();
    dp_exp.set_value(1.5);
    let ec_inv = ec.inverse();

    // Dynamic are not inverse.
    assert!(!ec.is_inverse(&ec_inv));

    assert_eq!(ec_inv.get_style(), ExposureContrastOpStyle::VideoRev);

    assert_eq!(ec.get_exposure(), ec_inv.get_exposure());
    assert_eq!(ec.get_contrast(), ec_inv.get_contrast());
    assert_eq!(ec.get_gamma(), ec_inv.get_gamma());
    assert_eq!(ec.get_pivot(), ec_inv.get_pivot());
    assert_eq!(ec.get_log_exposure_step(), ec_inv.get_log_exposure_step());
    assert_eq!(ec.get_log_mid_gray(), ec_inv.get_log_mid_gray());

    assert_eq!(
        ec.get_exposure_property().is_dynamic(),
        ec_inv.get_exposure_property().is_dynamic()
    );
    assert_eq!(
        ec.get_contrast_property().is_dynamic(),
        ec_inv.get_contrast_property().is_dynamic()
    );
    assert_eq!(
        ec.get_gamma_property().is_dynamic(),
        ec_inv.get_gamma_property().is_dynamic()
    );

    // Inverse makes a copy of the dynamic property rather than sharing the original.
    dp_exp.set_value(0.21);
    assert_ne!(ec.get_exposure(), ec_inv.get_exposure());
    assert_eq!(ec.get_exposure(), 0.21);
    assert_eq!(ec_inv.get_exposure(), 1.5);

    // Exposure is dynamic in both, never equal.
    assert!(!ec.is_inverse(&ec_inv));

    ec_inv.get_contrast_property().make_dynamic();

    // Contrast is dynamic in one and not in the other.
    assert!(!ec.is_inverse(&ec_inv));

    ec.get_contrast_property().make_dynamic();
    assert!(!ec.is_inverse(&ec_inv));

    // Gamma values are now different.
    ec.set_gamma(1.2);
    assert!(!ec.is_inverse(&ec_inv));
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOpData, equality)` @ v2.5.2.
#[test]
fn equality() {
    let mut ec0 = ExposureContrastOpData::default();
    let mut ec1 = ExposureContrastOpData::default();
    assert!(ec0 == ec1);

    // Change style.
    ec0.set_style(ExposureContrastOpStyle::Video);
    assert!(!(ec0 == ec1));
    ec1.set_style(ExposureContrastOpStyle::Video);
    assert!(ec0 == ec1);

    // Change dynamic.
    ec0.get_exposure_property().make_dynamic();
    assert!(!(ec0 == ec1));
    ec1.get_exposure_property().make_dynamic();
    assert!(!(ec0 == ec1));

    // Change value of enabled dynamic property.
    ec0.set_exposure(0.5);
    assert!(!(ec0 == ec1));
    ec1.set_exposure(0.5);
    assert!(!(ec0 == ec1));

    // Change value of dynamic property not enabled.
    ec1.set_contrast(0.5);
    assert!(!(ec0 == ec1));

    ec0.set_contrast(0.5);
    assert!(!(ec0 == ec1));
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOpData, replace_dynamic_property)` @ v2.5.2.
#[test]
fn replace_dynamic_property() {
    let ec0 = ExposureContrastOpData::default();
    let mut ec1 = ExposureContrastOpData::default();

    ec0.set_exposure(0.0);
    ec1.set_exposure(1.0);

    ec0.get_exposure_property().make_dynamic();
    ec1.get_exposure_property().make_dynamic();

    let dpe0 = ec0
        .get_dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();
    let mut dpe1 = ec1
        .get_dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();

    // These are 2 different pointers.
    assert!(!Arc::ptr_eq(double_of(&dpe0), double_of(&dpe1)));

    let dpd0 = ec0.get_exposure_property();
    ec1.replace_dynamic_property(DynamicPropertyType::Exposure, dpd0)
        .unwrap();
    dpe1 = ec1
        .get_dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();

    // Now, this is the same pointer.
    assert!(Arc::ptr_eq(double_of(&dpe0), double_of(&dpe1)));

    ec0.get_contrast_property().make_dynamic();
    // Contrast is not enabled in ec1.
    assert!(
        ec1.get_dynamic_property(DynamicPropertyType::Contrast)
            .is_err()
    );

    // The property is not replaced if dynamic is not enabled.
    let dpd0 = ec0.get_contrast_property();
    assert!(
        ec1.replace_dynamic_property(DynamicPropertyType::Contrast, dpd0)
            .is_err()
    );
    assert!(
        ec1.get_dynamic_property(DynamicPropertyType::Contrast)
            .is_err()
    );
}
