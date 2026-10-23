// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the exposure and contrast transform:
//! `tests/cpu/transforms/ExposureContrastTransform_tests.cpp` @ v2.5.2, and `ExposureContrastOp
//! create_transform` (tests/cpu/ops/exposurecontrast/ExposureContrastOp_tests.cpp @ v2.5.2),
//! which tests `CreateExposureContrastTransform` and needed the transform. The text, the
//! validation, the equality, the YAML and the ops built are compared with the wheel's in
//! `tests/exposure_contrast_transform_oracle.rs` and the YAML oracle tests.

use ocio_ops::format_metadata::METADATA_NAME;
use ocio_ops::logging::{get_logging_level, set_logging_level};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{DynamicPropertyType, LoggingLevel, TransformDirection};
use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::config::Config;
use crate::transform::Transform;

/// Port of `OCIO_ADD_TEST(ExposureContrastTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut ec = ExposureContrastTransform::new();
    assert_eq!(ec.direction(), TransformDirection::Forward);
    assert_eq!(ec.style(), ExposureContrastStyle::Linear);
    assert_eq!(ec.exposure(), 0.0);
    assert_eq!(ec.contrast(), 1.0);
    assert_eq!(ec.gamma(), 1.0);
    assert_eq!(ec.pivot(), 0.18);
    assert_eq!(
        ec.log_exposure_step(),
        ExposureContrastOpData::LOGEXPOSURESTEP_DEFAULT
    );
    assert_eq!(
        ec.log_mid_gray(),
        ExposureContrastOpData::LOGMIDGRAY_DEFAULT
    );
    ec.validate().unwrap();

    ec.set_direction(TransformDirection::Inverse);
    assert_eq!(ec.direction(), TransformDirection::Inverse);
    ec.validate().unwrap();

    ec.set_style(ExposureContrastStyle::Logarithmic);
    assert_eq!(ec.style(), ExposureContrastStyle::Logarithmic);
    ec.validate().unwrap();

    ec.set_style(ExposureContrastStyle::Video);
    assert_eq!(ec.style(), ExposureContrastStyle::Video);
    ec.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(ExposureContrastTransform, processor)` @ v2.5.2.
#[test]
fn processor() {
    let config = Config::new().unwrap();

    let mut ec = ExposureContrastTransform::new();
    ec.set_style(ExposureContrastStyle::Video);
    ec.set_exposure(1.1);
    ec.make_exposure_dynamic();
    ec.set_contrast(0.5);
    ec.set_gamma(1.5);

    let processor = config
        .processor(&Transform::ExposureContrast(ec.clone()))
        .unwrap();
    let cpu_processor = processor.default_cpu_processor().unwrap();

    let mut pixel = [0.2f32, 0.3, 0.4];
    cpu_processor.apply_rgb(&mut pixel).unwrap();

    let error = 1e-5f32;
    check_close(pixel[0], 0.32340, error);
    check_close(pixel[1], 0.43834, error);
    check_close(pixel[2], 0.54389, error);

    // Changing the original transform does not change the processor.
    ec.set_exposure(2.1);

    pixel = [0.2, 0.3, 0.4];
    cpu_processor.apply_rgb(&mut pixel).unwrap();
    check_close(pixel[0], 0.32340, error);
    check_close(pixel[1], 0.43834, error);
    check_close(pixel[2], 0.54389, error);

    let dp_exposure = cpu_processor
        .get_dynamic_property(DynamicPropertyType::Exposure)
        .unwrap();
    let dp_val = dp_exposure.as_double().unwrap();
    dp_val.set_value(2.1);

    // Gamma is a property of ExposureContrast but here it is not defined as dynamic.
    assert!(
        cpu_processor
            .get_dynamic_property(DynamicPropertyType::Gamma)
            .is_err()
    );

    // Processor has been changed by dpExposure.
    pixel = [0.2, 0.3, 0.4];
    cpu_processor.apply_rgb(&mut pixel).unwrap();
    check_close(pixel[0], 0.42965, error);
    check_close(pixel[1], 0.58235, error);
    check_close(pixel[2], 0.72258, error);

    // dpExposure can be used to change the processor.
    dp_val.set_value(0.8);
    pixel = [0.2, 0.3, 0.4];
    cpu_processor.apply_rgb(&mut pixel).unwrap();
    check_close(pixel[0], 0.29698, error);
    // Adjust error for SSE approximation.
    check_close(pixel[1], 0.40252, error * 2.0);
    check_close(pixel[2], 0.49946, error);
}

/// Port of `OCIO_ADD_TEST(ExposureContrastTransform, processor_several_ec)` @ v2.5.2.
///
/// This test verifies that if there are several ops in a processor that contain a given
/// dynamic property that
/// 1) Only the op for which the dynamic property is enabled responds to changes in the
///    property, and
/// 2) Ops where a given dynamic property is not enabled continue to use the initial value and
///    do not respond to changes to the property, and
/// 3) If two ops enable the same dynamic property an exception is thrown.
#[test]
fn processor_several_ec() {
    let config = Config::new().unwrap();

    //
    // Build expected values using 2 E/C with no dynamic parameters.
    //

    // 2 different exposure values.
    let a = 1.1;
    let b = 2.1;
    let mut ec1 = ExposureContrastTransform::new();
    ec1.set_style(ExposureContrastStyle::Logarithmic);

    ec1.set_exposure(a);
    ec1.set_contrast(0.5);
    ec1.set_gamma(1.5);

    let src_pixel = [0.2f32, 0.3, 0.4];

    // Will hold results for exposure a.
    let mut pixel_a = src_pixel;
    // Will hold results for exposure a applied twice.
    let mut pixel_aa;
    {
        let processor = config
            .processor(&Transform::ExposureContrast(ec1.clone()))
            .unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_a).unwrap();

        pixel_aa = pixel_a;
        cpu_processor.apply_rgb(&mut pixel_aa).unwrap();
    }

    let mut ec2 = ExposureContrastTransform::new();
    ec2.set_style(ExposureContrastStyle::Logarithmic);

    ec2.set_exposure(b);
    ec2.set_contrast(0.5);
    ec2.set_gamma(1.5);

    // Will hold results for exposure b.
    let mut pixel_b = src_pixel;
    // Will hold results for exposure b applied twice.
    let mut pixel_bb;
    // Will hold results for exposure a applied then exposure b applied.
    let mut pixel_ab = pixel_a;
    {
        let processor = config
            .processor(&Transform::ExposureContrast(ec2.clone()))
            .unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_b).unwrap();

        pixel_bb = pixel_b;

        cpu_processor.apply_rgb(&mut pixel_bb).unwrap();
        cpu_processor.apply_rgb(&mut pixel_ab).unwrap();
    }
    let _ = pixel_bb;

    // Make exposure of second E/C dynamic.
    ec2.make_exposure_dynamic();
    let error = 1e-6f32;

    //
    // Test with two E/C where only the second one is dynamic.
    //
    let mut grp1 = GroupTransform::new();
    ec2.set_exposure(a);
    grp1.append_transform(ec1.clone().into()); // ec1 exposure is a
    grp1.append_transform(ec2.clone().into()); // ec2 exposure is a

    {
        let processor = config.processor(&Transform::Group(grp1)).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();

        // Second exposure is dynamic. Value is still a.
        let dp_exposure = cpu_processor
            .get_dynamic_property(DynamicPropertyType::Exposure)
            .unwrap();
        let dp_val = dp_exposure.as_double().unwrap();

        let mut pixel = src_pixel;

        // Apply a then a.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_aa[0], error);
        check_close(pixel[1], pixel_aa[1], error);
        check_close(pixel[2], pixel_aa[2], error);

        // Change the 2nd exposure.
        dp_val.set_value(b);
        pixel = src_pixel;

        // Apply a then b.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_ab[0], error);
        check_close(pixel[1], pixel_ab[1], error);
        check_close(pixel[2], pixel_ab[2], error);
    }

    //
    // Test with two E/C where both are dynamic.
    //

    {
        // Make exposure of first E/C dynamic (second one already is).
        ec1.make_exposure_dynamic();

        let mut grp2 = GroupTransform::new();
        grp2.append_transform(ec1.into());
        grp2.append_transform(ec2.into());

        let (processor, log) = crate::test_env::capture_log(|| {
            let level = get_logging_level();
            set_logging_level(LoggingLevel::Warning);
            let processor = config.processor(&Transform::Group(grp2));
            set_logging_level(level);
            processor
        });
        processor.unwrap();
        assert_eq!(
            String::from_utf8_lossy(&log.concat()),
            "[OpenColorIO Warning]: Exposure dynamic property can only be there once.\n"
        );
    }
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;
    let mut data = ExposureContrastOpData::default();

    data.get_contrast_property().make_dynamic();
    data.set_exposure(1.2);
    data.set_pivot(0.5);
    data.set_log_exposure_step(0.09);
    data.set_log_mid_gray(0.7);
    data.get_format_metadata_mut()
        .add_attribute(Some(METADATA_NAME), Some(b"test"))
        .unwrap();

    let mut ops = OpVec::new();
    create_exposure_contrast_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_exposure_contrast_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::ExposureContrast(ec_transform) = transform else {
        panic!("not an ExposureContrastTransform: {transform:?}");
    };

    let metadata = ec_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), b"name");
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(ec_transform.direction(), TransformDirection::Forward);

    assert_eq!(ec_transform.exposure(), data.get_exposure());
    assert!(!ec_transform.is_exposure_dynamic());
    assert_eq!(ec_transform.contrast(), data.get_contrast());
    assert!(ec_transform.is_contrast_dynamic());
    assert_eq!(ec_transform.gamma(), data.get_gamma());
    assert!(!ec_transform.is_gamma_dynamic());
    assert_eq!(ec_transform.pivot(), data.get_pivot());
    assert_eq!(
        ec_transform.log_exposure_step(),
        data.get_log_exposure_step()
    );
    assert_eq!(ec_transform.log_mid_gray(), data.get_log_mid_gray());
}

/// `CreateExposureContrastTransform`'s refusal of another op type
/// (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOp.cpp:199-203 @ v2.5.2).
#[test]
fn create_transform_of_another_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::matrix::matrix_op::create_scale_op(
        &mut ops,
        &[2.0, 2.0, 2.0, 1.0],
        TransformDirection::Forward,
    );
    let mut group = GroupTransform::new();
    check_throw_what(
        create_exposure_contrast_transform(&mut group, &ops[0]),
        "CreateExposureContrastTransform: op has to be a ExposureContrastOp",
    );
    assert_eq!(group.num_transforms(), 0);
}
