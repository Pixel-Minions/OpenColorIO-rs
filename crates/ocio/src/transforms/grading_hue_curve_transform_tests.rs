// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the hue curves grading transform:
//! `tests/cpu/transforms/GradingHueCurveTransform_tests.cpp` @ v2.5.2; `GradingHueCurveOp
//! create_transform` and `build_ops` (tests/cpu/ops/gradinghuecurve/GradingHueCurveOp_tests.cpp
//! @ v2.5.2), which test `CreateGradingHueCurveTransform` and `BuildGradingHueCurveOp`; and
//! `DynamicPropertyImpl setter_validation` (tests/cpu/DynamicProperty_tests.cpp @ v2.5.2),
//! which builds a processor of the transform. The text, the validation, the YAML and the ops
//! built are compared with the wheel's in `tests/grading_hue_curve_transform_oracle.rs` and the
//! YAML oracle tests.

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

/// A curve of `c`'s spline type through the points `(x, y)`, as
/// `GradingBSplineCurve::Create({...}, c)` makes it.
fn curve(points: &[(f32, f32)], c: HueCurveType) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = points
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points_and_hue_curve_type(&points, c)
}

/// Sets the control points of curve `c` of `hue_curve` to `points`, as the tests do with
/// `setNumControlPoints` and `getControlPoint(i) = ...`.
fn set_points(hue_curve: &mut GradingHueCurve, c: HueCurveType, points: &[(f32, f32)]) {
    let spline = hue_curve.curve_mut(c).unwrap();
    spline.set_num_control_points(points.len());
    for (i, &(x, y)) in points.iter().enumerate() {
        *spline.control_point_mut(i).unwrap() = GradingControlPoint::new(x, y);
    }
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    // Create transform and validate default values for all styles.

    let mut gct_lin = GradingHueCurveTransform::new(GradingStyle::Lin);
    assert_eq!(gct_lin.style(), GradingStyle::Lin);
    assert_eq!(gct_lin.direction(), TransformDirection::Forward);
    gct_lin.set_direction(TransformDirection::Inverse);
    assert_eq!(gct_lin.direction(), TransformDirection::Inverse);
    assert_eq!(gct_lin.rgb_to_hsy(), HsyTransformStyle::Transform1);
    assert!(!gct_lin.is_dynamic());
    let value = gct_lin.value();
    let crv = value.curve(HueCurveType::LumSat).unwrap();
    assert_eq!(crv.num_control_points(), 3);
    assert_eq!(
        *crv.control_point(0).unwrap(),
        GradingControlPoint::new(-7.0, 1.0)
    );
    assert_eq!(
        *crv.control_point(1).unwrap(),
        GradingControlPoint::new(0.0, 1.0)
    );
    assert_eq!(
        *crv.control_point(2).unwrap(),
        GradingControlPoint::new(7.0, 1.0)
    );
    let crv = value.curve(HueCurveType::HueLum).unwrap();
    assert_eq!(value.curve(HueCurveType::HueSat).unwrap(), crv);
    gct_lin.validate().unwrap();

    let gct_log = GradingHueCurveTransform::new(GradingStyle::Log);
    assert_eq!(gct_log.style(), GradingStyle::Log);
    assert_eq!(gct_log.direction(), TransformDirection::Forward);
    assert_eq!(gct_log.rgb_to_hsy(), HsyTransformStyle::Transform1);
    assert!(!gct_log.is_dynamic());
    let log_value = gct_log.value();
    let crv = log_value.curve(HueCurveType::LumSat).unwrap();
    assert_eq!(crv.num_control_points(), 3);
    assert_eq!(
        *crv.control_point(0).unwrap(),
        GradingControlPoint::new(0.0, 1.0)
    );
    assert_eq!(
        *crv.control_point(1).unwrap(),
        GradingControlPoint::new(0.5, 1.0)
    );
    assert_eq!(
        *crv.control_point(2).unwrap(),
        GradingControlPoint::new(1.0, 1.0)
    );
    let crv = log_value.curve(HueCurveType::HueLum).unwrap();
    assert_eq!(log_value.curve(HueCurveType::HueSat).unwrap(), crv);
    let crv = log_value.curve(HueCurveType::LumLum).unwrap();
    assert_eq!(log_value.curve(HueCurveType::SatSat).unwrap(), crv);
    gct_log.validate().unwrap();

    let gct_vid = GradingHueCurveTransform::new(GradingStyle::Video);
    assert_eq!(gct_vid.style(), GradingStyle::Video);
    assert_eq!(gct_vid.direction(), TransformDirection::Forward);
    assert_eq!(gct_vid.rgb_to_hsy(), HsyTransformStyle::Transform1);
    assert!(!gct_vid.is_dynamic());
    let vid_value = gct_vid.value();
    let crv = vid_value.curve(HueCurveType::LumSat).unwrap();
    assert_eq!(crv.num_control_points(), 3);
    assert_eq!(
        *crv.control_point(0).unwrap(),
        GradingControlPoint::new(0.0, 1.0)
    );
    assert_eq!(
        *crv.control_point(1).unwrap(),
        GradingControlPoint::new(0.5, 1.0)
    );
    assert_eq!(
        *crv.control_point(2).unwrap(),
        GradingControlPoint::new(1.0, 1.0)
    );
    let crv = log_value.curve(HueCurveType::HueLum).unwrap();
    assert_eq!(log_value.curve(HueCurveType::HueSat).unwrap(), crv);
    let crv = log_value.curve(HueCurveType::LumLum).unwrap();
    assert_eq!(log_value.curve(HueCurveType::SatSat).unwrap(), crv);
    gct_vid.validate().unwrap();

    // Change values.
    let t = Transform::from(gct_vid).create_editable_copy().unwrap();
    let Transform::GradingHueCurve(mut gct) = t else {
        panic!("not a GradingHueCurveTransform");
    };
    gct.set_style(GradingStyle::Lin);
    assert_eq!(gct.style(), GradingStyle::Lin);
    gct.set_direction(TransformDirection::Inverse);
    assert_eq!(gct.direction(), TransformDirection::Inverse);
    gct.set_rgb_to_hsy(HsyTransformStyle::None);
    assert_eq!(gct.rgb_to_hsy(), HsyTransformStyle::None);
    gct.make_dynamic();
    assert!(gct.is_dynamic());
    gct.set_value(&gct_lin.value()).unwrap();
    let value = gct.value();
    let crv = value.curve(HueCurveType::LumLum).unwrap();
    assert_eq!(
        *crv.control_point(0).unwrap(),
        GradingControlPoint::new(-7.0, -7.0)
    );
    gct.validate().unwrap();

    // Access out of range point.
    check_throw_what(
        crv.control_point(4),
        "There are '3' control points. '4' is out of bounds.",
    );

    // X-coordinate has to be increasing.
    {
        let mut hct = GradingHueCurveTransform::new(GradingStyle::Video);
        let mut hue_curve = hct.value();
        let lumsat = hue_curve.curve_mut(HueCurveType::LumSat).unwrap();
        *lumsat.control_point_mut(0).unwrap() = GradingControlPoint::new(0.7, 1.0);
        check_throw_what(
            hct.set_value(&hue_curve),
            "has a x coordinate '0.5' that is less than previous control point x coordinate \
             '0.7'.",
        );
    }

    // Y-coordinate has to be increasing, for diagonal curves.
    {
        let mut hct = GradingHueCurveTransform::new(GradingStyle::Video);
        let mut hue_curve = hct.value();
        let lumlum = hue_curve.curve_mut(HueCurveType::LumLum).unwrap();
        *lumlum.control_point_mut(0).unwrap() = GradingControlPoint::new(0.0, 0.6);
        check_throw_what(
            hct.set_value(&hue_curve),
            "has a y coordinate '0.5' that is less than previous control point y coordinate \
             '0.6'.",
        );
    }

    // Check slopes.
    gct.set_slope(HueCurveType::LumLum, 2, 0.9).unwrap();
    gct.validate().unwrap();
    assert_eq!(gct.slope(HueCurveType::LumLum, 2).unwrap(), 0.9f32);
    check_throw_what(
        gct.set_slope(HueCurveType::LumLum, 4, 2.0),
        "There are '3' control points. '4' is out of bounds.",
    );
    assert!(gct.slopes_are_default(HueCurveType::LumSat).unwrap());
    assert!(!gct.slopes_are_default(HueCurveType::LumLum).unwrap());
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveTransform, processor_several_transforms)` @ v2.5.2.
#[test]
fn processor_several_transforms() {
    let mut gcta = GradingHueCurveTransform::new(GradingStyle::Log);
    let mut hue_curve_a = gcta.value();
    // Shift all hues up by 0.1.
    set_points(
        &mut hue_curve_a,
        HueCurveType::HueFx,
        &[(0.0, 0.1), (0.9, 0.1)],
    );
    gcta.set_value(&hue_curve_a).unwrap();

    gcta.validate().unwrap();
    assert!(!hue_curve_a.is_identity());

    let config = Config::new().unwrap();
    let src_pixel = [0.2f32, 0.3, 0.4];

    // Will hold results for hueCurveA.
    let mut pixel_a = src_pixel;
    // Will hold results for hueCurveA applied twice.
    let mut pixel_aa;
    {
        let processor = config.processor(&gcta.clone().into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_a).unwrap();

        pixel_aa = pixel_a;
        cpu_processor.apply_rgb(&mut pixel_aa).unwrap();
    }

    // NB: This must be GRADING_LOG like above because the test will be changing the curves
    // as dynamic parameters but that does not change the base style.
    let mut gctb = GradingHueCurveTransform::new(GradingStyle::Log);
    let mut hue_curve_b = gctb.value();
    // Increase sat at all luminances by 1.5.
    set_points(
        &mut hue_curve_b,
        HueCurveType::LumSat,
        &[(0.0, 1.5), (1.0, 1.5)],
    );
    gctb.set_value(&hue_curve_b).unwrap();

    assert!(!hue_curve_b.is_identity());

    // Will hold results for hueCurveA applied then hueCurveB applied.
    let mut pixel_ab = pixel_a;
    {
        let processor = config.processor(&gctb.clone().into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();
        cpu_processor.apply_rgb(&mut pixel_ab).unwrap();
    }

    // Make second transform dynamic.
    gctb.make_dynamic();
    let error = 1e-6f32;

    //
    // Test with two grading hue curve transforms where only the second one is dynamic.
    //

    let mut grp1 = GroupTransform::new();
    gctb.set_value(&hue_curve_a).unwrap();
    grp1.append_transform(gcta.clone().into()); // gpta values are hueCurveA
    grp1.append_transform(gctb.clone().into()); // gptb values are hueCurveA

    {
        let processor = config.processor(&grp1.into()).unwrap();
        let cpu_processor = processor.default_cpu_processor().unwrap();

        // Second transform is dynamic. Value is still hueCurveA.
        let dp = cpu_processor
            .get_dynamic_property(DynamicPropertyType::GradingHueCurve)
            .unwrap();
        let dp_val = dp.as_grading_hue_curve().unwrap();

        let mut pixel = src_pixel;

        // Apply hueCurveA then hueCurveA.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_aa[0], error);
        check_close(pixel[1], pixel_aa[1], error);
        check_close(pixel[2], pixel_aa[2], error);

        // Change the 2nd values.
        dp_val.set_value(&hue_curve_b).unwrap();

        pixel = src_pixel;

        // Apply hueCurveA then hueCurveB.
        cpu_processor.apply_rgb(&mut pixel).unwrap();

        check_close(pixel[0], pixel_ab[0], error);
        check_close(pixel[1], pixel_ab[1], error);
        check_close(pixel[2], pixel_ab[2], error);
    }

    //
    // Test two grading hue curve transforms can't be both dynamic.
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
            "[OpenColorIO Warning]: Grading hue curve dynamic property can only be there once.\n"
        );
    }
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveTransform, serialization)` @ v2.5.2.
#[test]
fn serialization() {
    // Test the serialization of the transform.

    let hh = curve(
        &[
            (0.1, -0.05),
            (0.2, 0.23),
            (0.5, 0.25),
            (0.8, 0.7),
            (0.85, 0.8),
            (0.95, 0.9),
        ],
        HueCurveType::HueHue,
    );
    let hs = curve(
        &[
            (0.0, 1.2),
            (0.1, 1.2),
            (0.4, 0.7),
            (0.6, 0.3),
            (0.8, 0.5),
            (0.9, 0.8),
        ],
        HueCurveType::HueSat,
    );
    let hl = curve(
        &[(0.1, 1.4), (0.2, 1.4), (0.4, 0.7), (0.6, 0.5), (0.8, 0.8)],
        HueCurveType::HueLum,
    );
    let ls = curve(
        &[(0.0, 1.0), (0.5, 1.5), (1.0, 0.9), (1.1, 1.1)],
        HueCurveType::LumSat,
    );
    let mut ss = curve(
        &[(0.0, 0.05), (0.5, 0.8), (1.0, 1.05)],
        HueCurveType::SatSat,
    );
    let ll = curve(
        &[(0.0, -0.0005), (0.5, 0.3), (1.0, 0.9)],
        HueCurveType::LumLum,
    );
    let sl = curve(&[(0.05, 1.1), (0.3, 1.0), (1.2, 0.9)], HueCurveType::SatLum);
    let hfx = curve(
        &[
            (-0.15, 0.1),
            (0.0, -0.05),
            (0.2, -0.1),
            (0.4, 0.3),
            (0.6, 0.25),
            (0.8, 0.2),
            (0.9, 0.05),
            (1.1, -0.07),
        ],
        HueCurveType::HueFx,
    );

    ss.set_slope(0, 1.2).unwrap();
    ss.set_slope(1, 0.8).unwrap();
    ss.set_slope(2, 0.4).unwrap();

    let data = GradingHueCurve::with_curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap();

    let mut curve = GradingHueCurveTransform::new(GradingStyle::Video);
    curve.validate().unwrap();

    curve.set_value(&data).unwrap();

    const CURVE_STR: &str = concat!(
        "<GradingHueCurveTransform direction=forward, style=video, values=",
        "<hue_hue=<control_points=[<x=0.1, y=-0.05><x=0.2, y=0.23><x=0.5, y=0.25><x=0.8, y=0.7>",
        "<x=0.85, y=0.8><x=0.95, y=0.9>]>, hue_sat=<control_points=[<x=0, y=1.2><x=0.1, y=1.2>",
        "<x=0.4, y=0.7><x=0.6, y=0.3><x=0.8, y=0.5><x=0.9, y=0.8>]>, hue_lum=<control_points=",
        "[<x=0.1, y=1.4><x=0.2, y=1.4><x=0.4, y=0.7><x=0.6, y=0.5><x=0.8, y=0.8>]>, lum_sat=",
        "<control_points=[<x=0, y=1><x=0.5, y=1.5><x=1, y=0.9><x=1.1, y=1.1>]>, sat_sat=<control_points=",
        "[<x=0, y=0.05, slp=1.2><x=0.5, y=0.8, slp=0.8><x=1, y=1.05, slp=0.4>]>, lum_lum=<control_points=",
        "[<x=0, y=-0.0005><x=0.5, y=0.3><x=1, y=0.9>]>, sat_lum=<control_points=[<x=0.05, y=1.1>",
        "<x=0.3, y=1><x=1.2, y=0.9>]>, hue_fx=<control_points=[<x=-0.15, y=0.1><x=0, y=-0.05>",
        "<x=0.2, y=-0.1><x=0.4, y=0.3><x=0.6, y=0.25><x=0.8, y=0.2><x=0.9, y=0.05><x=1.1, y=-0.07>]>>>"
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

/// Port of `OCIO_ADD_TEST(GradingHueCurveTransform, local_bypass)` @ v2.5.2.
#[test]
fn local_bypass() {
    // Test that the GPU shader is empty for an identity transform.

    let transform = GradingHueCurveTransform::new(GradingStyle::Log);

    transform.validate().unwrap();

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

/// Port of `OCIO_ADD_TEST(GradingHueCurveOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;
    let data = GradingHueCurveOpData::new(GradingStyle::Log);
    data.get_dynamic_property_internal().make_dynamic();
    let mut ops = OpVec::new();

    create_grading_hue_curve_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_grading_hue_curve_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::GradingHueCurve(gc_transform) = transform else {
        panic!("not a GradingHueCurveTransform: {transform:?}");
    };
    assert_eq!(gc_transform.style(), GradingStyle::Log);
    assert!(gc_transform.is_dynamic());
}

/// `CreateGradingHueCurveTransform`'s refusal of another op type
/// (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOp.cpp:221-225 @ v2.5.2).
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
        create_grading_hue_curve_transform(&mut group, &ops[0]),
        "CreateGradingHueCurveTransform: op has to be a GradingHueCurveOp.",
    );
    assert_eq!(group.num_transforms(), 0);
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOp, build_ops)` @ v2.5.2.
#[test]
fn build_ops_test() {
    let config = Config::create_raw().unwrap();

    let mut gc_transform = GradingHueCurveTransform::new(GradingStyle::Log);

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
    assert_eq!(ops[0].get_info(), "<GradingHueCurveOp>");
    let OpData::GradingHueCurve(gcd) = &**ops[0].data() else {
        panic!("not a GradingHueCurve op");
    };
    assert!(gcd.is_dynamic());

    let mut vals_op = gcd.get_value();
    assert_eq!(
        6,
        vals_op
            .curve(HueCurveType::HueHue)
            .unwrap()
            .num_control_points()
    );

    // Create processor with dynamic identity before changing the transform.
    let proc = config.processor(&gc_transform.clone().into()).unwrap();
    assert!(proc.has_dynamic_property(DynamicPropertyType::GradingHueCurve));
    assert!(!proc.has_dynamic_property(DynamicPropertyType::Exposure));

    let cpu = proc.default_cpu_processor().unwrap();

    // Create a non-identity curve.
    let mut hue_curve = GradingHueCurve::new(GradingStyle::Log);
    // Add a constant offset to all hues.
    set_points(
        &mut hue_curve,
        HueCurveType::HueLum,
        &[(0.0, 2.0), (0.9, 2.0)],
    );
    assert!(!hue_curve.is_identity());

    // Sharing of dynamic properties is done through processor, changing the source transform
    // does not change the op that was created from it.
    gc_transform.set_value(&hue_curve).unwrap();
    vals_op = gcd.get_value();
    // (Still has its original value.)
    assert_eq!(
        1.0f32,
        vals_op
            .curve(HueCurveType::HueLum)
            .unwrap()
            .control_point(0)
            .unwrap()
            .y
    );

    // Get dynamic property from the CPU processor.
    let dp = cpu
        .get_dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap();
    // Get typed value accessor.
    let dpgc = dp.as_grading_hue_curve().unwrap();

    let mut pixel = [0.0f32, 0.2, 2.0];
    cpu.apply_rgb(&mut pixel).unwrap();
    // Default values are identity.
    let error = 1e-5f32;
    check_close(pixel[0], 0.0f32, error);
    check_close(pixel[1], 0.2f32, error);
    check_close(pixel[2], 2.0f32, error);

    // Set the modified curve.
    dpgc.set_value(&hue_curve).unwrap();

    // The pixel value is different.
    cpu.apply_rgb(&mut pixel).unwrap();
    check_close(pixel[0], 0.1f32, error);
    check_close(pixel[1], 0.3f32, error);
    check_close(pixel[2], 2.1f32, error);
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, setter_validation)` @ v2.5.2. Upstream's first
/// three checks compare each channel with itself.
#[test]
fn setter_validation() {
    // Make an identity dynamic transform.
    let mut gct = GradingHueCurveTransform::new(GradingStyle::Log);
    gct.make_dynamic();

    // Apply it on CPU.
    let config = Config::new().unwrap();
    let processor = config.processor(&gct.into()).unwrap();
    let cpu_processor = processor.default_cpu_processor().unwrap();

    let mut pixel = [0.4f32, 0.3, 0.2];
    cpu_processor.apply_rgb(&mut pixel).unwrap();

    let error = 1e-5f32;
    check_close(pixel[0], pixel[0], error);
    check_close(pixel[1], pixel[1], error);
    check_close(pixel[2], pixel[2], error);

    // Get a handle to the dynamic property.
    let dp = cpu_processor
        .get_dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap();
    let dp_val = dp.as_grading_hue_curve().unwrap();

    // Set a non-identity value.
    let mut hue_curve = dp_val.get_value();
    set_points(
        &mut hue_curve,
        HueCurveType::HueHue,
        &[(0.0, -0.1), (0.5, 0.5), (0.8, 0.8)],
    );
    dp_val.set_value(&hue_curve).unwrap();
    cpu_processor.apply_rgb(&mut pixel).unwrap();

    check_close(pixel[0], 0.4385873675f32, error);
    check_close(pixel[1], 0.2829087377f32, error);
    check_close(pixel[2], 0.2556785941f32, error);

    // Ensure that validation of control points is happening as expected. Set the last point
    // so that it is no longer monotonic with respect to the first point. Because it is
    // periodic, the last point Y value becomes -0.05 when wrapped around to an X value of -0.2.
    *hue_curve
        .curve_mut(HueCurveType::HueHue)
        .unwrap()
        .control_point_mut(2)
        .unwrap() = GradingControlPoint::new(0.8, 0.95);
    check_throw_what(
        dp_val.set_value(&hue_curve),
        "GradingHueCurve validation failed for 'hue_hue' curve with: Control point at index 0 \
         has a y coordinate '-0.1' that is less than previous control point y coordinate \
         '-0.05'.",
    );
}

/// `set_value_as_is` (the Python binding's hook) holds curves that don't validate as they are:
/// the transform gives them back and refuses them where it validates, and building ops from
/// it refuses them as building ops from curves set with `set_value` would (the copy's fit).
/// (The binding's oracle tests compare what Python sees with the wheel's.)
#[test]
fn curves_held_as_they_are() {
    let mut t = GradingHueCurveTransform::new(GradingStyle::Log);
    let mut values = t.value();
    *values.curve_mut(HueCurveType::LumSat).unwrap() = curve(
        &[(0.0, 1.0), (0.5, 1.0), (0.2, 1.0), (1.0, 1.0)],
        HueCurveType::LumSat,
    );
    assert!(values.validate().is_err());
    assert!(t.set_value(&values).is_err());
    t.set_value_as_is(&values);
    assert_eq!(t.value(), values);
    assert!(t.validate().is_err());
    let config = Config::create_raw().unwrap();
    assert!(config.processor(&t.clone().into()).is_err());
    // Valid curves held as they are build the same ops as curves set.
    let mut valid = t.value();
    *valid.curve_mut(HueCurveType::LumSat).unwrap() =
        curve(&[(0.0, 1.2), (0.5, 0.8), (1.0, 1.0)], HueCurveType::LumSat);
    let mut set = GradingHueCurveTransform::new(GradingStyle::Log);
    set.set_value(&valid).unwrap();
    t.set_value_as_is(&valid);
    let apply = |tr: Transform| {
        let cpu = config
            .processor(&tr)
            .unwrap()
            .default_cpu_processor()
            .unwrap();
        let mut px = [0.25f32, 0.5, 0.75];
        cpu.apply_rgb(&mut px).unwrap();
        px.map(f32::to_bits)
    };
    let (a, b) = (apply(t.clone().into()), apply(set.clone().into()));
    assert_ne!(
        a,
        [0.25f32, 0.5, 0.75].map(f32::to_bits),
        "the curves change the pixel"
    );
    assert_eq!(a, b);
}
