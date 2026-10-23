// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the tone grading transform: `tests/cpu/transforms/GradingToneTransform_tests.cpp`
//! @ v2.5.2, and `GradingToneOp create_transform` and `build_ops`
//! (tests/cpu/ops/gradingtone/GradingToneOp_tests.cpp @ v2.5.2), which test
//! `CreateGradingToneTransform` and `BuildGradingToneOp` and needed the transform. The text,
//! the validation, the YAML and the ops built are compared with the wheel's in
//! `tests/grading_tone_transform_oracle.rs` and the YAML oracle tests.

use ocio_gpu::GpuShaderDesc;
use ocio_ops::open_color_types::{DynamicPropertyType, OptimizationFlags, TransformDirection};
use ocio_ops::ops::gradingtone::grading_tone::GradingRgbmsw;
use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::config::Config;
use crate::transform::{Transform, build_ops};

/// Port of `OCIO_ADD_TEST(GradingToneTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    // Create transform and validate default values for all styles.
    let mut gtt_lin = GradingToneTransform::new(GradingStyle::Lin);
    assert_eq!(gtt_lin.style(), GradingStyle::Lin);

    let tone_defaults_lin = GradingTone::new(GradingStyle::Lin);
    assert_eq!(gtt_lin.value(), tone_defaults_lin);
    let mut tone = GradingTone::new(GradingStyle::Lin);
    tone.scontrast += 0.123;
    tone.blacks.red += 0.321;
    tone.blacks.start += 0.1;
    gtt_lin.set_value(&tone).unwrap();
    assert_eq!(gtt_lin.value(), tone);

    assert_eq!(gtt_lin.direction(), TransformDirection::Forward);
    gtt_lin.set_direction(TransformDirection::Inverse);
    assert_eq!(gtt_lin.direction(), TransformDirection::Inverse);

    assert!(!gtt_lin.is_dynamic());
    gtt_lin.make_dynamic();
    assert!(gtt_lin.is_dynamic());
    gtt_lin.make_non_dynamic();
    assert!(!gtt_lin.is_dynamic());

    gtt_lin.validate().unwrap();
    tone.blacks.width = 0.0001;
    check_throw_what(gtt_lin.set_value(&tone), "is below lower bound (0.01)");
    tone.blacks.width = 1.;
    tone.blacks.red = 2.1;
    check_throw_what(gtt_lin.set_value(&tone), "are above upper bound (1.9)");

    let gtt_log = GradingToneTransform::new(GradingStyle::Log);
    assert_eq!(gtt_log.style(), GradingStyle::Log);
    let tone_defaults_log = GradingTone::new(GradingStyle::Log);
    assert_eq!(gtt_log.value(), tone_defaults_log);
    let gtt_vid = GradingToneTransform::new(GradingStyle::Video);
    assert_eq!(gtt_vid.style(), GradingStyle::Video);
    let tone_defaults_vid = GradingTone::new(GradingStyle::Video);
    assert_eq!(gtt_vid.value(), tone_defaults_vid);
}

/// Port of `OCIO_ADD_TEST(GradingToneTransform, serialization)` @ v2.5.2.
#[test]
fn serialization() {
    // Test the serialization of the transform.

    let mut data = GradingTone::new(GradingStyle::Lin);
    data.scontrast += 0.123;
    data.blacks.red += 0.321;
    data.blacks.start += 0.1;

    let mut tone = GradingToneTransform::new(GradingStyle::Lin);
    tone.set_value(&data).unwrap();

    const TONE_STR: &str = concat!(
        "<GradingToneTransform direction=forward, style=linear, values=<",
        "blacks=<red=1.321 green=1 blue=1 master=1 start=0.1 width=4> ",
        "shadows=<red=1 green=1 blue=1 master=1 start=2 width=-7> ",
        "midtones=<red=1 green=1 blue=1 master=1 start=0 width=8> ",
        "highlights=<red=1 green=1 blue=1 master=1 start=-2 width=9> ",
        "whites=<red=1 green=1 blue=1 master=1 start=0 width=8> s_contrast=1.123>>"
    );

    assert_eq!(tone.to_string(), TONE_STR);

    let mut grp = GroupTransform::new();
    grp.append_transform(tone.into());

    let mut group_str = String::from("<GroupTransform direction=forward, transforms=\n");
    group_str += "        ";
    group_str += TONE_STR;
    group_str += ">";

    assert_eq!(grp.to_string(), group_str);
}

/// Port of `OCIO_ADD_TEST(GradingToneTransform, local_bypass)` @ v2.5.2. Upstream's
/// `OCIO_CHECK_CLOSE_FROM` with `std::numeric_limits<float>::max_digits10` passes the
/// integer 9 as the tolerance.
#[test]
fn local_bypass() {
    // Test the local bypass behavior.

    let mut transform = GradingToneTransform::new(GradingStyle::Log);

    let config = Config::create_raw().unwrap();

    {
        // Test that the GPU is empty for an identity transform.

        let proc = config.processor(&transform.clone().into()).unwrap();
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

    {
        transform.make_dynamic();

        let proc = config.processor(&transform.into()).unwrap();
        let cpu = proc
            .optimized_cpu_processor(OptimizationFlags::NONE)
            .unwrap();

        // `std::numeric_limits<float>::max_digits10`.
        let digits = 9.0f32;

        // Values are unchanged (to within 32f precision).

        let mut v1 = [0.3f32, 0.4, 0.5];
        cpu.apply_rgb(&mut v1).unwrap();
        check_close(v1[0], 0.30000001192092896f32, digits);
        check_close(v1[1], 0.4000000059604645f32, digits);
        assert_eq!(v1[2], 0.5f32);

        // Try a value > HalfMax = 65504, note it is not clamped.  Therefore localBypass is
        // being used.

        let mut v2 = [0.3f32, 0.4, 65550.0];
        cpu.apply_rgb(&mut v2).unwrap();
        check_close(v2[0], 0.30000001192092896f32, digits);
        check_close(v2[1], 0.4000000059604645f32, digits);
        assert_eq!(v2[2], 65550.0f32);

        // Set the midtones control so it is no longer an identity.

        let mut valst = GradingTone::new(GradingStyle::Log);
        valst.midtones = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.2, 0.37, 0.6);
        let dp = cpu
            .get_dynamic_property(DynamicPropertyType::GradingTone)
            .unwrap();
        let prop_gt = dp.as_grading_tone().unwrap();
        prop_gt.set_value(&valst).unwrap();

        // It is affecting the midtones.

        let mut v3 = [0.3f32, 0.4, 0.5];
        cpu.apply_rgb(&mut v3).unwrap();
        check_close(v3[0], 0.20410963892936707f32, digits);
        check_close(v3[1], 0.4343799948692322f32, digits);
        check_close(v3[2], 0.6093657612800598f32, digits);

        // The max value is now clamped, so localBypass is not being used.

        let mut v4 = [0.3f32, 0.4, 65550.0];
        cpu.apply_rgb(&mut v4).unwrap();
        check_close(v4[0], 0.20410963892936707f32, digits);
        check_close(v4[1], 0.4343799948692322f32, digits);
        assert_eq!(v4[2], 65504.0f32);

        // The midtones control does not affect large values, so just clarify that large values
        // below 65504 are not affected.

        let mut v5 = [0.3f32, 0.4, 65500.0];
        cpu.apply_rgb(&mut v5).unwrap();
        check_close(v5[0], 0.20410963892936707f32, digits);
        check_close(v5[1], 0.4343799948692322f32, digits);
        assert_eq!(v5[2], 65500.0f32);

        // Set the midtones values back to its default.

        valst.midtones = GradingRgbmsw::new(1., 1.0, 1., 1., 0.4, 0.6);
        prop_gt.set_value(&valst).unwrap();

        // The behavior is now the same as originally.

        let mut v6 = [0.3f32, 0.4, 0.5];
        cpu.apply_rgb(&mut v6).unwrap();
        check_close(v6[0], 0.30000001192092896f32, digits);
        check_close(v6[1], 0.4000000059604645f32, digits);
        assert_eq!(v6[2], 0.5f32);

        let mut v7 = [0.3f32, 0.4, 65550.0];
        cpu.apply_rgb(&mut v7).unwrap();
        check_close(v7[0], 0.30000001192092896f32, digits);
        check_close(v7[1], 0.4000000059604645f32, digits);
        assert_eq!(v7[2], 65550.0f32);
    }
}

/// Port of `OCIO_ADD_TEST(GradingToneOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;
    let data = GradingToneOpData::new(GradingStyle::Log);
    data.get_dynamic_property_internal().make_dynamic();
    let mut ops = OpVec::new();

    data.get_dynamic_property_internal().make_dynamic();
    create_grading_tone_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_grading_tone_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::GradingTone(gt_transform) = transform else {
        panic!("not a GradingToneTransform: {transform:?}");
    };
    assert_eq!(gt_transform.style(), GradingStyle::Log);
    assert!(gt_transform.is_dynamic());
}

/// `CreateGradingToneTransform`'s refusal of another op type
/// (src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:219-223 @ v2.5.2).
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
        create_grading_tone_transform(&mut group, &ops[0]),
        "CreateGradingToneTransform: op has to be a GradingToneOp.",
    );
    assert_eq!(group.num_transforms(), 0);
}

/// Port of `OCIO_ADD_TEST(GradingToneOp, build_ops)` @ v2.5.2.
#[test]
fn build_ops_test() {
    let config = Config::create_raw().unwrap();

    let mut gt_transform = GradingToneTransform::new(GradingStyle::Log);

    // Identity does create an op.
    let mut ops = OpVec::new();
    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &gt_transform.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_identity().unwrap());
    assert!(ops[0].is_no_op().unwrap());
    ops.clear();

    // Make it dynamic and keep default values.
    gt_transform.make_dynamic();

    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &gt_transform.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();

    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<GradingToneOp>");
    let OpData::GradingTone(gtd) = &**ops[0].data() else {
        panic!("not a GradingTone op");
    };
    assert!(gtd.is_dynamic());

    let mut vals_op = gtd.get_value();
    assert_eq!(vals_op.scontrast, 1.);

    // Sharing of dynamic properties is done through processor, changing the source will not
    // change the op.
    let mut vals = GradingTone::new(GradingStyle::Log);
    vals.scontrast = 1.1;
    gt_transform.set_value(&vals).unwrap();

    vals_op = gtd.get_value();
    assert_eq!(vals_op.scontrast, 1.);

    let proc = config.processor(&gt_transform.into()).unwrap();
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingTone));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));

    let cpu = proc.default_cpu_processor().unwrap();

    // Get dynamic property from the CPU proc.
    let dp = cpu
        .get_dynamic_property(DynamicPropertyType::GradingTone)
        .unwrap();
    // Get typed value accessor.
    let dpgt = dp.as_grading_tone().unwrap();

    // Set identity value in dynamic property.
    vals.scontrast = 1.0;
    dpgt.set_value(&vals).unwrap();

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel).unwrap();
    // Default values are identity.
    assert_eq!(pixel[0], 0.0f32);
    assert_eq!(pixel[1], 0.2f32);
    assert_eq!(pixel[2], 2.0f32);

    // Change values and update dynamic property.
    vals.scontrast = 1.1;
    vals.midtones.red = 1.1;
    dpgt.set_value(&vals).unwrap();

    // No longer identity.
    cpu.apply_rgb(&mut pixel).unwrap();

    let error = 1e-5f32;
    check_close(pixel[0], 0.0f32, error);
    check_close(pixel[1], 0.18729f32, error);
    check_close(pixel[2], 1.91875f32, error);
}
