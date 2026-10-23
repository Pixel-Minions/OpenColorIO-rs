// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the primary grading transform:
//! `tests/cpu/transforms/GradingPrimaryTransform_tests.cpp` @ v2.5.2, and `GradingPrimaryOp
//! create_transform` and `build_ops` (tests/cpu/ops/gradingprimary/GradingPrimaryOp_tests.cpp
//! @ v2.5.2), which test `CreateGradingPrimaryTransform` and `BuildGradingPrimaryOp` and needed
//! the transform. The text, the validation, the equality, the YAML and the ops built are
//! compared with the wheel's in `tests/grading_primary_transform_oracle.rs` and the YAML oracle
//! tests.

use ocio_gpu::GpuShaderDesc;
use ocio_ops::logging::{get_logging_level, set_logging_level};
use ocio_ops::math_utils::vecs_equal_with_rel_error;
use ocio_ops::open_color_types::{
    DynamicPropertyType, LoggingLevel, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::gradingprimary::grading_primary::GradingRgbm;
use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::config::Config;
use crate::transform::{Transform, build_ops};

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    assert_eq!(GradingPrimary::NO_CLAMP_WHITE, f64::MAX);
    assert_eq!(GradingPrimary::NO_CLAMP_BLACK, -f64::MAX);

    // Verify default data.
    let gdp_lin = GradingPrimary::new(GradingStyle::Lin);

    assert_eq!(gdp_lin.brightness, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.contrast, GradingRgbm::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(gdp_lin.gamma, GradingRgbm::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(gdp_lin.offset, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.exposure, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.lift, GradingRgbm::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(gdp_lin.gain, GradingRgbm::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(gdp_lin.pivot, 0.18);
    assert_eq!(gdp_lin.saturation, 1.0);
    assert_eq!(gdp_lin.clamp_white, GradingPrimary::NO_CLAMP_WHITE);
    assert_eq!(gdp_lin.clamp_black, GradingPrimary::NO_CLAMP_BLACK);
    assert_eq!(gdp_lin.pivot_white, 1.0);
    assert_eq!(gdp_lin.pivot_black, 0.0);

    // Log defaults only differ lin defaults by pivot value.
    let gdp_log = GradingPrimary::new(GradingStyle::Log);
    assert_ne!(gdp_log, gdp_lin);
    assert_eq!(gdp_log.pivot, -0.2);
    let mut gdp_log_edit = gdp_log;
    assert_eq!(gdp_log, gdp_log_edit);
    gdp_log_edit.pivot = gdp_lin.pivot;
    assert_eq!(gdp_log_edit, gdp_lin);

    // Video defaults are the same as lin defaults.
    let gdp_vid = GradingPrimary::new(GradingStyle::Video);
    assert_eq!(gdp_vid, gdp_lin);

    // Create transform and validate default values for all styles.
    let gpt_lin = GradingPrimaryTransform::new(GradingStyle::Lin);
    assert_eq!(gpt_lin.style(), GradingStyle::Lin);
    assert_eq!(gpt_lin.direction(), TransformDirection::Forward);
    assert_eq!(gpt_lin.value(), gdp_lin);
    gpt_lin.validate().unwrap();

    let gpt_log = GradingPrimaryTransform::new(GradingStyle::Log);
    assert_eq!(gpt_log.style(), GradingStyle::Log);
    assert_eq!(gpt_log.value(), gdp_log);
    gpt_log.validate().unwrap();

    let gpt_vid = GradingPrimaryTransform::new(GradingStyle::Video);
    assert_eq!(gpt_vid.style(), GradingStyle::Video);
    assert_eq!(gpt_vid.value(), gdp_vid);
    gpt_vid.validate().unwrap();

    // Create editable copy and change values.
    let t = Transform::from(gpt_lin.clone())
        .create_editable_copy()
        .unwrap();
    let Transform::GradingPrimary(mut gpt) = t else {
        panic!("not a GradingPrimaryTransform");
    };
    assert_eq!(gpt.style(), GradingStyle::Lin);
    assert_eq!(gpt.direction(), TransformDirection::Forward);
    assert_eq!(gpt.value(), gdp_lin);

    gpt.set_direction(TransformDirection::Inverse);
    assert_eq!(gpt.direction(), TransformDirection::Inverse);

    gpt.set_style(GradingStyle::Video);
    assert_eq!(gpt.style(), GradingStyle::Video);

    let mut v = gpt.value();
    v.pivot = 0.24;
    gpt.set_value(&v).unwrap();
    assert_eq!(gpt.value().pivot, 0.24);

    gpt.set_style(GradingStyle::Log);
    gpt.set_direction(TransformDirection::Forward);
    let gamma_invalid = GradingRgbm::new(0.00001, 1.0, 1.0, 1.0);
    v.gamma = gamma_invalid;
    check_throw_what(
        gpt.set_value(&v),
        "GradingPrimary gamma '<r=1e-05, g=1, b=1, m=1>' are below lower bound (0.01)",
    );
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, dynamic)` @ v2.5.2.
#[test]
fn dynamic() {
    let mut gpt = GradingPrimaryTransform::new(GradingStyle::Log);

    assert!(!gpt.is_dynamic());
    gpt.make_dynamic();
    assert!(gpt.is_dynamic());
}

/// The messages logged on this thread while `f` runs at the warning level, as upstream's
/// `LogGuard` with `SetLoggingLevel(LOGGING_LEVEL_WARNING)` (tests/cpu/UnitTestLogUtils.cpp
/// @ v2.5.2) collects them.
fn warning_output<T>(f: impl FnOnce() -> T) -> (T, String) {
    let (out, log) = crate::test_env::capture_log(|| {
        let level = get_logging_level();
        set_logging_level(LoggingLevel::Warning);
        let out = f();
        set_logging_level(level);
        out
    });
    (out, String::from_utf8_lossy(&log.concat()).into_owned())
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, processor_several_transforms)` @ v2.5.2.
#[test]
fn processor_several_transforms() {
    let config = Config::new().unwrap();
    let src_pixel = [0.2f32, 0.3, 0.4];

    let mut gpa = GradingPrimary::new(GradingStyle::Log);
    gpa.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let mut gpta = GradingPrimaryTransform::new(GradingStyle::Log);
    gpta.set_value(&gpa).unwrap();

    // Will hold results for gpa.
    let mut pixel_a = src_pixel;
    // Will hold results for gpa applied twice.
    let mut pixel_aa;
    {
        let processor = config.processor(&gpta.clone().into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_a).unwrap();

        pixel_aa = pixel_a;
        cpu_processor.apply_rgb(&mut pixel_aa).unwrap();
    }

    let mut gpb = GradingPrimary::new(GradingStyle::Log);
    gpb.gamma = GradingRgbm::new(1.2, 1.4, 1.1, 1.0);
    gpb.saturation = 1.5;
    let mut gptb = GradingPrimaryTransform::new(GradingStyle::Log);
    gptb.set_value(&gpb).unwrap();

    // Will hold results for gpb.
    let mut pixel_b = src_pixel;
    // Will hold results for gpb applied twice.
    let mut pixel_bb;
    // Will hold results for gpa applied then gpb applied.
    let mut pixel_ab = pixel_a;
    {
        let processor = config.processor(&gptb.clone().into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_b).unwrap();

        pixel_bb = pixel_b;

        cpu_processor.apply_rgb(&mut pixel_bb).unwrap();
        cpu_processor.apply_rgb(&mut pixel_ab).unwrap();
    }
    let _ = pixel_bb;

    // Make second transform dynamic.
    gptb.make_dynamic();
    let error = 1e-6f32;

    //
    // Test with two grading primary transforms where only the second one is dynamic.
    //
    let mut grp1 = GroupTransform::new();
    gptb.set_value(&gpa).unwrap();
    grp1.append_transform(gpta.clone().into()); // gpta values are gpa
    grp1.append_transform(gptb.clone().into()); // gptb values are gpa

    {
        let processor = config.processor(&grp1.into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();

        // Second transform is dynamic. Value is still gpa.
        let dp = cpu_processor
            .get_dynamic_property(DynamicPropertyType::GradingPrimary)
            .unwrap();
        let dp_val = dp.as_grading_primary().unwrap();

        let mut pixel = src_pixel;

        // Apply gpa then gpa.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_aa[0], error);
        check_close(pixel[1], pixel_aa[1], error);
        check_close(pixel[2], pixel_aa[2], error);

        // Change the 2nd values to gpb.
        dp_val.set_value(&gpb).unwrap();
        pixel = src_pixel;

        // Apply gpa then gpb.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_ab[0], error);
        check_close(pixel[1], pixel_ab[1], error);
        check_close(pixel[2], pixel_ab[2], error);
    }

    //
    // Test that two grading primary transforms can't be both dynamic.
    //

    // Make first transform dynamic (second already is).
    gpta.make_dynamic();

    let mut grp2 = GroupTransform::new();
    grp2.append_transform(gpta.into());
    grp2.append_transform(gptb.into());

    {
        let (processor, log) = warning_output(|| config.processor(&grp2.into()));
        processor.unwrap();
        assert_eq!(
            log,
            "[OpenColorIO Warning]: Grading primary dynamic property can only be there once.\n"
        );
    }
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, several_transforms_switch)` @ v2.5.2.
#[test]
fn several_transforms_switch() {
    let config = Config::new().unwrap();
    let src_pixel = [0.2f32, 0.3, 0.4];

    let mut gpa = GradingPrimary::new(GradingStyle::Log);
    gpa.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);
    let mut gpta = GradingPrimaryTransform::new(GradingStyle::Log);
    gpta.set_value(&gpa).unwrap();

    let mut gpb = GradingPrimary::new(GradingStyle::Log);
    gpb.gamma = GradingRgbm::new(1.2, 1.4, 1.1, 1.0);
    gpb.saturation = 1.5;
    let mut gptb = GradingPrimaryTransform::new(GradingStyle::Log);
    gptb.set_value(&gpb).unwrap();

    // To start, make only the first transform dynamic.
    gpta.make_dynamic();

    //
    // Test with two grading primary transforms where first is dynamic.
    //
    let mut grp = GroupTransform::new();
    grp.append_transform(gpta.clone().into());
    grp.append_transform(gptb.clone().into());

    let processor = config.processor(&grp.into()).unwrap();
    let cpu_processor = processor.default_cpu_processor().unwrap();

    let error = 1e-6f32;
    let mut pixel = src_pixel;
    {
        // The dynamic property is for the first op.
        let dp = cpu_processor
            .get_dynamic_property(DynamicPropertyType::GradingPrimary)
            .unwrap();
        let dp_val = dp.as_grading_primary().unwrap();

        // Change one property and apply to the source pixel.
        let mut val = dp_val.get_value();
        val.saturation += 0.15;
        dp_val.set_value(&val).unwrap();

        let mut temp = src_pixel;
        cpu_processor.apply_rgb(&mut temp).unwrap();

        // Change additional properties.
        val.brightness.red += 0.11;
        val.gamma.master += 0.123456;
        dp_val.set_value(&val).unwrap();

        // Apply to the same source pixel (and keep results).
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        // Additional properties were modified so the temp and pixel results are different.
        assert!(!vecs_equal_with_rel_error(&temp, &pixel, error));
    }

    // Now make the first transform non-dynamic and the second transform dynamic.  This is what
    // an application would need to do in order to edit multiple different dynamic transforms.

    {
        // Get dynamic property from the current cpuProcessor, to retrieve the current value.
        let dp = cpu_processor
            .get_dynamic_property(DynamicPropertyType::GradingPrimary)
            .unwrap();
        let dp_val = dp.as_grading_primary().unwrap();
        let val = dp_val.get_value();

        // Copy the newly edited property values back to the first transform.
        gpta.set_value(&val).unwrap();

        // Swap which transform is dynamic.
        gpta.make_non_dynamic();
        gptb.make_dynamic();
    }

    // Create a new processor (since it is not possible to make a transform dynamic after the
    // processor is created).
    let mut grp = GroupTransform::new();
    grp.append_transform(gpta.into());
    grp.append_transform(gptb.into());

    let processor = config.processor(&grp.into()).unwrap();
    let cpu_processor = processor.default_cpu_processor().unwrap();

    let mut pixel2 = src_pixel;
    {
        // The application is now able to edit the second transform.  The first transform has
        // been updated to reflect the editing done above.

        cpu_processor.apply_rgb(&mut pixel2).unwrap();

        // Check that the new processor is getting the same result as the latest edits on the
        // previous processor.
        check_close(pixel[0], pixel2[0], error);
        check_close(pixel[1], pixel2[1], error);
        check_close(pixel[2], pixel2[2], error);

        // Get a pointer to the dynamic property in the processor, this time it is on the
        // second transform.
        let dp = cpu_processor
            .get_dynamic_property(DynamicPropertyType::GradingPrimary)
            .unwrap();
        let dp_val = dp.as_grading_primary().unwrap();

        // Change properties.
        let mut val = dp_val.get_value();
        val.saturation += 0.15;
        dp_val.set_value(&val).unwrap();

        let mut temp = src_pixel;
        cpu_processor.apply_rgb(&mut temp).unwrap();

        // Additional properties were modified so the temp and pixel results are different.
        assert!(!vecs_equal_with_rel_error(&temp, &pixel, error));
    }
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, serialization)` @ v2.5.2.
#[test]
fn serialization() {
    // Test the serialization of the transform.

    let mut data = GradingPrimary::new(GradingStyle::Log);
    data.gamma = GradingRgbm::new(1.1, 1.2, 1.3, 1.0);

    let mut primary = GradingPrimaryTransform::new(GradingStyle::Log);
    primary.set_value(&data).unwrap();

    const PRIMARY_STR: &str = concat!(
        "<GradingPrimaryTransform direction=forward, style=log, ",
        "values=<brightness=<r=0, g=0, b=0, m=0>, contrast=<r=1, g=1, b=1, m=1>, ",
        "gamma=<r=1.1, g=1.2, b=1.3, m=1>, offset=<r=0, g=0, b=0, m=0>, ",
        "exposure=<r=0, g=0, b=0, m=0>, lift=<r=0, g=0, b=0, m=0>, ",
        "gain=<r=1, g=1, b=1, m=1>, saturation=1, pivot=<contrast=-0.2, black=0, white=1>>>"
    );

    assert_eq!(primary.to_string(), PRIMARY_STR);

    let mut grp = GroupTransform::new();
    grp.append_transform(primary.into());

    let mut group_str = String::from("<GroupTransform direction=forward, transforms=\n");
    group_str += "        ";
    group_str += PRIMARY_STR;
    group_str += ">";

    assert_eq!(grp.to_string(), group_str);
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, log_contrast_inverse_apply)` @ v2.5.2.
#[test]
fn log_contrast_inverse_apply() {
    let cfg = Config::create_raw().unwrap();

    let mut data = GradingPrimary::new(GradingStyle::Log);
    data.contrast = GradingRgbm::new(1.1, 0.9, 1.2, 1.0);

    let mut primary = GradingPrimaryTransform::new(GradingStyle::Log);
    primary.set_value(&data).unwrap();

    let proc = cfg.processor(&primary.clone().into()).unwrap();
    let cpu = proc.default_cpu_processor().unwrap();
    let pixel_ref = [0.0f32, 0.0, 0.0];
    let mut pixel = [0.0f32, 0.0, 0.0];
    cpu.apply_rgb(&mut pixel).unwrap();

    let proc2 = cfg
        .processor_in_direction(&primary.clone().into(), TransformDirection::Inverse)
        .unwrap();
    let cpu2 = proc2.default_cpu_processor().unwrap();
    cpu2.apply_rgb(&mut pixel).unwrap();

    let error = 1e-6f32;
    check_close(pixel[0], pixel_ref[0], error);
    check_close(pixel[1], pixel_ref[1], error);
    check_close(pixel[2], pixel_ref[2], error);

    pixel = pixel_ref;
    cpu.apply_rgb(&mut pixel).unwrap();

    primary.set_direction(TransformDirection::Inverse);
    let proc3 = cfg.processor(&primary.into()).unwrap();
    let cpu3 = proc3.default_cpu_processor().unwrap();
    cpu3.apply_rgb(&mut pixel).unwrap();

    check_close(pixel[0], pixel_ref[0], error);
    check_close(pixel[1], pixel_ref[1], error);
    check_close(pixel[2], pixel_ref[2], error);
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryTransform, local_bypass)` @ v2.5.2.
#[test]
fn local_bypass() {
    // Test that the GPU is empty for an identity transform.

    let transform = GradingPrimaryTransform::new(GradingStyle::Log);

    let config = Config::create_raw().unwrap();

    let proc = config.processor(&transform.into()).unwrap();
    let gpu = proc
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();

    let mut shader_desc = GpuShaderDesc::default();

    gpu.extract_gpu_shader_info(&mut shader_desc).unwrap();

    let gpu_str = concat!(
        "\n",
        "// Declaration of the OCIO shader function\n",
        "\n",
        "vec4 OCIOMain(vec4 inPixel)\n",
        "{\n",
        "  vec4 outColor = inPixel;\n",
        "\n",
        "  return outColor;\n",
        "}\n"
    );

    assert_eq!(
        gpu_str,
        std::str::from_utf8(shader_desc.shader_text()).unwrap()
    );
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;
    let data = GradingPrimaryOpData::new(GradingStyle::Log);
    data.get_dynamic_property_internal().make_dynamic();
    let mut ops = OpVec::new();
    create_grading_primary_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_grading_primary_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::GradingPrimary(gp_transform) = transform else {
        panic!("not a GradingPrimaryTransform: {transform:?}");
    };
    assert_eq!(gp_transform.style(), GradingStyle::Log);
    assert!(gp_transform.is_dynamic());
}

/// `CreateGradingPrimaryTransform`'s refusal of another op type
/// (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOp.cpp:225-229 @ v2.5.2).
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
        create_grading_primary_transform(&mut group, &ops[0]),
        "CreateGradingPrimaryTransform: op has to be a GradingPrimaryOp.",
    );
    assert_eq!(group.num_transforms(), 0);
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOp, build_ops)` @ v2.5.2.
#[test]
fn build_ops_test() {
    let config = Config::create_raw().unwrap();

    let mut gp_transform = GradingPrimaryTransform::new(GradingStyle::Log);

    // Identity does create an op.
    let mut ops = OpVec::new();
    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &gp_transform.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_identity().unwrap());
    assert!(ops[0].is_no_op().unwrap());
    ops.clear();

    // Make it dynamic and keep default values.
    gp_transform.make_dynamic();

    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &gp_transform.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();

    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<GradingPrimaryOp>");
    let OpData::GradingPrimary(gpd) = &**ops[0].data() else {
        panic!("not a GradingPrimary op");
    };
    assert!(gpd.is_dynamic());

    let mut vals_op = gpd.get_value();
    assert_eq!(vals_op.pivot_black, 0.);

    // Sharing of dynamic properties is done through processor, changing the source will not
    // change the op.
    let mut vals = GradingPrimary::new(GradingStyle::Log);
    vals.pivot_black = 0.1;
    gp_transform.set_value(&vals).unwrap();

    vals_op = gpd.get_value();
    assert_eq!(vals_op.pivot_black, 0.);

    let proc = config.processor(&gp_transform.into()).unwrap();
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingPrimary));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));
    check_throw_what(
        proc.dynamic_property(DynamicPropertyType::Exposure),
        "Cannot find dynamic property",
    );

    let cpu = proc.default_cpu_processor().unwrap();

    // Get dynamic property from the CPU proc.
    let dp = cpu
        .get_dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    // Get typed value accessor.
    let dpgp = dp.as_grading_primary().unwrap();

    check_throw_what(
        cpu.get_dynamic_property(DynamicPropertyType::Exposure),
        "Cannot find dynamic property",
    );

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel).unwrap();
    // Default values are identity.
    let error = 1e-5f32;
    check_close(pixel[0], 0.0f32, error);
    check_close(pixel[1], 0.2f32, error);
    check_close(pixel[2], 2.0f32, 5.0 * error);

    // Add clamping and update dynamic property.
    vals.clamp_black = 0.1;
    vals.clamp_white = 1.;
    dpgp.set_value(&vals).unwrap();

    // Values are now clamped.
    cpu.apply_rgb(&mut pixel).unwrap();
    check_close(pixel[0], 0.1f32, error);
    check_close(pixel[1], 0.2f32, error);
    check_close(pixel[2], 1.0f32, error);
}
