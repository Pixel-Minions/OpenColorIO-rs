// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the RGB curves grading transform:
//! `tests/cpu/transforms/GradingRGBCurveTransform_tests.cpp` @ v2.5.2, and `GradingRGBCurveOp
//! create_transform` and `build_ops` (tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOp_tests.cpp
//! @ v2.5.2), which test `CreateGradingRGBCurveTransform` and `BuildGradingRGBCurveOp` and
//! needed the transform. The text, the validation, the YAML and the ops built are compared with
//! the wheel's in `tests/grading_rgb_curve_transform_oracle.rs` and the YAML oracle tests.

use ocio_gpu::GpuShaderDesc;
use ocio_ops::logging::{get_logging_level, set_logging_level};
use ocio_ops::open_color_types::{
    DynamicPropertyType, LoggingLevel, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::config::Config;
use crate::transform::{Transform, build_ops};

/// A curve through the points `(x, y)`, as `GradingBSplineCurve::Create({...})` makes it.
fn curve(points: &[(f32, f32)]) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = points
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points(&points)
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    // Create transform and validate default values for all styles.

    let gct_lin = GradingRgbCurveTransform::new(GradingStyle::Lin);
    assert_eq!(gct_lin.style(), GradingStyle::Lin);
    assert_eq!(gct_lin.direction(), TransformDirection::Forward);
    assert!(!gct_lin.bypass_lin_to_log());
    assert!(!gct_lin.is_dynamic());
    let value = gct_lin.value();
    let red = value.curve(RgbCurveType::Red).unwrap();
    assert_eq!(red.num_control_points(), 3);
    assert_eq!(
        *red.control_point(0).unwrap(),
        GradingControlPoint::new(-7.0, -7.0)
    );
    assert_eq!(
        *red.control_point(1).unwrap(),
        GradingControlPoint::new(0.0, 0.0)
    );
    assert_eq!(
        *red.control_point(2).unwrap(),
        GradingControlPoint::new(7.0, 7.0)
    );
    assert_eq!(value.curve(RgbCurveType::Green).unwrap(), red);
    assert_eq!(value.curve(RgbCurveType::Blue).unwrap(), red);
    assert_eq!(value.curve(RgbCurveType::Master).unwrap(), red);
    gct_lin.validate().unwrap();

    for style in [GradingStyle::Log, GradingStyle::Video] {
        let gct = GradingRgbCurveTransform::new(style);
        assert_eq!(gct.style(), style);
        assert_eq!(gct.direction(), TransformDirection::Forward);
        assert!(!gct.bypass_lin_to_log());
        assert!(!gct.is_dynamic());
        let value = gct.value();
        let red = value.curve(RgbCurveType::Red).unwrap();
        assert_eq!(red.num_control_points(), 3);
        assert_eq!(
            *red.control_point(0).unwrap(),
            GradingControlPoint::new(0.0, 0.0)
        );
        assert_eq!(
            *red.control_point(1).unwrap(),
            GradingControlPoint::new(0.5, 0.5)
        );
        assert_eq!(
            *red.control_point(2).unwrap(),
            GradingControlPoint::new(1.0, 1.0)
        );
        assert_eq!(value.curve(RgbCurveType::Green).unwrap(), red);
        assert_eq!(value.curve(RgbCurveType::Blue).unwrap(), red);
        assert_eq!(value.curve(RgbCurveType::Master).unwrap(), red);
        gct.validate().unwrap();
    }

    // Change values.
    let gct_vid = GradingRgbCurveTransform::new(GradingStyle::Video);
    let t = Transform::from(gct_vid).create_editable_copy().unwrap();
    let Transform::GradingRgbCurve(mut gct) = t else {
        panic!("not a GradingRGBCurveTransform");
    };
    gct.set_style(GradingStyle::Lin);
    assert_eq!(gct.style(), GradingStyle::Lin);
    gct.set_direction(TransformDirection::Inverse);
    assert_eq!(gct.direction(), TransformDirection::Inverse);
    gct.set_bypass_lin_to_log(true);
    assert!(gct.bypass_lin_to_log());
    gct.make_dynamic();
    assert!(gct.is_dynamic());
    gct.set_value(&gct_lin.value()).unwrap();
    let value = gct.value();
    let red = value.curve(RgbCurveType::Red).unwrap().clone();
    assert_eq!(
        *red.control_point(0).unwrap(),
        GradingControlPoint::new(-7.0, -7.0)
    );
    gct.validate().unwrap();

    // Access out of range point.
    check_throw_what(
        red.control_point(4),
        "There are '3' control points. '4' is out of bounds.",
    );

    // X has to be increasing.
    let invalid_curve = curve(&[(0.0, 0.0), (0.5, 0.2), (0.2, 0.7), (1.0, 1.0)]);
    let new_curve = GradingRgbCurve::with_curves(&red, &red, &invalid_curve, &red);
    check_throw_what(
        gct.set_value(&new_curve),
        "has a x coordinate '0.2' that is less than previous control point x coordinate '0.5'.",
    );

    // Check slopes.
    gct.set_slope(RgbCurveType::Blue, 2, 0.9).unwrap();
    gct.validate().unwrap();
    assert_eq!(gct.slope(RgbCurveType::Blue, 2).unwrap(), 0.9f32);
    check_throw_what(
        gct.set_slope(RgbCurveType::Blue, 4, 2.0),
        "There are '3' control points. '4' is out of bounds.",
    );
    assert!(gct.slopes_are_default(RgbCurveType::Green).unwrap());
    assert!(!gct.slopes_are_default(RgbCurveType::Blue).unwrap());
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveTransform, processor_several_transforms)` @ v2.5.2.
#[test]
fn processor_several_transforms() {
    let config = Config::new().unwrap();
    let src_pixel = [0.2f32, 0.3, 0.4];

    let c1 = curve(&[(0.0, 0.0), (0.2, 0.2), (0.5, 0.7), (1.0, 1.0)]);
    let c2 = curve(&[(0.0, 0.5), (0.3, 0.7), (0.5, 1.1), (1.0, 1.5)]);
    let c3 = curve(&[
        (0.0, -0.5),
        (0.2, -0.4),
        (0.3, 0.1),
        (0.5, 0.4),
        (0.7, 0.9),
        (1.0, 1.1),
    ]);
    let c4 = curve(&[(-1.0, 0.0), (0.2, 0.2), (0.8, 0.8), (2.0, 1.0)]);
    let c5 = curve(&[(0.0, 0.0), (1.0, 1.0)]);

    let rgb_curve_a = GradingRgbCurve::with_curves(&c1, &c2, &c3, &c5);

    let mut gcta = GradingRgbCurveTransform::new(GradingStyle::Log);
    gcta.set_value(&rgb_curve_a).unwrap();

    // Will hold results for rgbCurveA.
    let mut pixel_a = src_pixel;
    // Will hold results for rgbCurveA applied twice.
    let mut pixel_aa;
    {
        let processor = config.processor(&gcta.clone().into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_a).unwrap();

        pixel_aa = pixel_a;
        cpu_processor.apply_rgb(&mut pixel_aa).unwrap();
    }

    let rgb_curve_b = GradingRgbCurve::with_curves(&c4, &c1, &c2, &c5);
    let mut gctb = GradingRgbCurveTransform::new(GradingStyle::Log);
    gctb.set_value(&rgb_curve_b).unwrap();

    // Will hold results for rgbCurveB.
    let mut pixel_b = src_pixel;
    // Will hold results for rgbCurveB applied twice.
    let mut pixel_bb;
    // Will hold results for rgbCurveA applied then rgbCurveB applied.
    let mut pixel_ab = pixel_a;
    {
        let processor = config.processor(&gctb.clone().into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_b).unwrap();

        pixel_bb = pixel_b;

        cpu_processor.apply_rgb(&mut pixel_bb).unwrap();
        cpu_processor.apply_rgb(&mut pixel_ab).unwrap();
    }
    let _ = pixel_bb;

    // Make second transform dynamic.
    gctb.make_dynamic();
    let error = 1e-6f32;

    //
    // Test with two grading rgb curve transforms where only the second one is dynamic.
    //
    let mut grp1 = GroupTransform::new();
    gctb.set_value(&rgb_curve_a).unwrap();
    grp1.append_transform(gcta.clone().into()); // gpta values are rgbCurveA
    grp1.append_transform(gctb.clone().into()); // gptb values are rgbCurveA

    {
        let processor = config.processor(&grp1.into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();

        // Second transform is dynamic. Value is still rgbCurveA.
        let dp = cpu_processor
            .get_dynamic_property(DynamicPropertyType::GradingRgbCurve)
            .unwrap();
        let dp_val = dp.as_grading_rgb_curve().unwrap();

        let mut pixel = src_pixel;

        // Apply rgbCurveA then rgbCurveA.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_aa[0], error);
        check_close(pixel[1], pixel_aa[1], error);
        check_close(pixel[2], pixel_aa[2], error);

        // Change the 2nd values.
        dp_val.set_value(&rgb_curve_b).unwrap();
        pixel = src_pixel;

        // Apply rgbCurveA then rgbCurveB.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_ab[0], error);
        check_close(pixel[1], pixel_ab[1], error);
        check_close(pixel[2], pixel_ab[2], error);
    }

    //
    // Test two grading rgb curve transforms can't be both dynamic.
    //

    // Make first dynamic (second already is).
    gcta.make_dynamic();

    let mut grp2 = GroupTransform::new();
    grp2.append_transform(gcta.into());
    grp2.append_transform(gctb.into());

    {
        let (processor, log) = crate::test_env::capture_log(|| {
            let level = get_logging_level();
            set_logging_level(LoggingLevel::Warning);
            let processor = config.processor(&grp2.into());
            set_logging_level(level);
            processor
        });
        processor.unwrap();
        assert_eq!(
            String::from_utf8_lossy(&log.concat()),
            "[OpenColorIO Warning]: Grading RGB curve dynamic property can only be there once.\n"
        );
    }
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveTransform, serialization)` @ v2.5.2.
#[test]
fn serialization() {
    // Test the serialization of the transform.

    let c1 = curve(&[(0.0, 0.0), (0.2, 0.2), (0.5, 0.7), (1.0, 1.0)]);
    let c2 = curve(&[(0.0, 0.5), (0.3, 0.7), (0.5, 1.1), (1.0, 1.5)]);
    let c3 = curve(&[
        (0.0, -0.5),
        (0.2, -0.4),
        (0.3, 0.1),
        (0.5, 0.4),
        (0.7, 0.9),
        (1.0, 1.1),
    ]);
    let c4 = curve(&[(0.0, 0.0), (1.0, 1.0)]);

    let data = GradingRgbCurve::with_curves(&c1, &c2, &c3, &c4);

    let mut curve = GradingRgbCurveTransform::new(GradingStyle::Log);
    curve.set_value(&data).unwrap();

    const CURVE_STR: &str = concat!(
        "<GradingRGBCurveTransform direction=forward, style=log, ",
        "values=<red=<control_points=[<x=0, y=0><x=0.2, y=0.2><x=0.5, y=0.7><x=1, y=1>]>, ",
        "green=<control_points=[<x=0, y=0.5><x=0.3, y=0.7><x=0.5, y=1.1><x=1, y=1.5>]>, ",
        "blue=<control_points=[<x=0, y=-0.5><x=0.2, y=-0.4><x=0.3, y=0.1><x=0.5, y=0.4><x=0.7, y=0.9><x=1, y=1.1>]>, ",
        "master=<control_points=[<x=0, y=0><x=1, y=1>]>>>"
    );

    assert_eq!(curve.to_string(), CURVE_STR);

    let mut grp = GroupTransform::new();
    grp.append_transform(curve.into());

    let mut group_str = String::from("<GroupTransform direction=forward, transforms=\n");
    group_str += "        ";
    group_str += CURVE_STR;
    group_str += ">";

    assert_eq!(grp.to_string(), group_str);
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveTransform, local_bypass)` @ v2.5.2.
#[test]
fn local_bypass() {
    // Test that the GPU is empty for an identity transform.

    let transform = GradingRgbCurveTransform::new(GradingStyle::Log);

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

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;
    let data = GradingRgbCurveOpData::new(GradingStyle::Log);
    data.get_dynamic_property_internal().make_dynamic();
    let mut ops = OpVec::new();

    create_grading_rgb_curve_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_grading_rgb_curve_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::GradingRgbCurve(gc_transform) = transform else {
        panic!("not a GradingRGBCurveTransform: {transform:?}");
    };
    assert_eq!(gc_transform.style(), GradingStyle::Log);
    assert!(gc_transform.is_dynamic());
}

/// `CreateGradingRGBCurveTransform`'s refusal of another op type
/// (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOp.cpp:225-229 @ v2.5.2).
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
        create_grading_rgb_curve_transform(&mut group, &ops[0]),
        "CreateGradingRGBCurveTransform: op has to be a GradingRGBCurveOp.",
    );
    assert_eq!(group.num_transforms(), 0);
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOp, build_ops)` @ v2.5.2.
#[test]
fn build_ops_test() {
    let config = Config::create_raw().unwrap();

    let mut gc_transform = GradingRgbCurveTransform::new(GradingStyle::Log);

    // Identity does create an op.
    let mut ops = OpVec::new();
    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &gc_transform.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_identity().unwrap());
    assert!(ops[0].is_no_op().unwrap());
    ops.clear();

    // Make it dynamic and keep default values.
    gc_transform.make_dynamic();

    build_ops(
        &mut ops,
        &config,
        &config.current_context().get(),
        &gc_transform.clone().into(),
        TransformDirection::Forward,
    )
    .unwrap();

    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<GradingRGBCurveOp>");
    let OpData::GradingRgbCurve(gcd) = &**ops[0].data() else {
        panic!("not a GradingRGBCurve op");
    };
    assert!(gcd.is_dynamic());

    let mut vals_op = gcd.get_value();
    assert_eq!(
        3,
        vals_op
            .curve(RgbCurveType::Green)
            .unwrap()
            .num_control_points()
    );

    // Create processor with dynamic identity before changing the transform.
    let proc = config.processor(&gc_transform.clone().into()).unwrap();
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingRgbCurve));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));

    let cpu = proc.default_cpu_processor().unwrap();

    // Sharing of dynamic properties is done through processor, changing the source will not
    // change the op.
    let curve = curve(&[(0.0, 0.1), (0.2, 0.3), (0.5, 0.8), (2.0, 1.5)]);
    let rgb_curve = GradingRgbCurve::with_curves(&curve, &curve, &curve, &curve);
    gc_transform.set_value(&rgb_curve).unwrap();

    // Still use the default identity curves.
    vals_op = gcd.get_value();
    assert_eq!(
        3,
        vals_op
            .curve(RgbCurveType::Green)
            .unwrap()
            .num_control_points()
    );

    // Get dynamic property from the CPU proc.
    let dp = cpu
        .get_dynamic_property(DynamicPropertyType::GradingRgbCurve)
        .unwrap();
    // Get typed value accessor.
    let dpgc = dp.as_grading_rgb_curve().unwrap();

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel).unwrap();
    // Default values are identity.
    let error = 1e-5f32;
    check_close(pixel[0], 0.0f32, error);
    check_close(pixel[1], 0.2f32, error);
    check_close(pixel[2], 2.0f32, error);

    // Use other curve that has 4 control points.
    dpgc.set_value(&rgb_curve).unwrap();

    // Control point has moved.
    cpu.apply_rgb(&mut pixel).unwrap();
    check_close(pixel[0], 0.18597151f32, error);
    check_close(pixel[1], 0.47056902f32, error);
    check_close(pixel[2], 1.32527864f32, error);
}
