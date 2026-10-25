// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/LegacyViewingPipeline_tests.cpp` @ v2.5.2.

use super::*;
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;
use crate::color_space::ColorSpace;
use crate::named_transform::NamedTransform;
use crate::transforms::exponent_transform::ExponentTransform;
use crate::transforms::exposure_contrast_transform::ExposureContrastTransform;
use crate::transforms::fixed_function_transform::FixedFunctionTransform;
use crate::transforms::log_transform::LogTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::view_transform::ViewTransform;
use ocio_ops::open_color_types::{
    ColorSpaceDirection, FixedFunctionStyle, ReferenceSpaceType, ViewTransformDirection,
};
use ocio_testkit::upstream::check_throw_what;

/// The transform of `variant` that `tr` is (upstream's `DynamicPtrCast`, then
/// `OCIO_REQUIRE_ASSERT` on it).
macro_rules! typed {
    ($tr:expr, $variant:ident) => {
        match $tr {
            Transform::$variant(t) => t,
            other => panic!(
                "expected a {} transform, got {:?}",
                stringify!($variant),
                other.transform_type()
            ),
        }
    };
}

/// The group transform of `proc` (`createGroupTransform`), validated.
fn group_of(proc: &Processor) -> GroupTransform {
    let group = proc.create_group_transform().unwrap();
    group.validate().unwrap();
    group
}

fn ff(style: FixedFunctionStyle) -> Transform {
    Transform::from(FixedFunctionTransform::new(style, &[]).unwrap())
}

/// Port of `OCIO_ADD_TEST(LegacyViewingPipeline, basic)` @ v2.5.2.
#[test]
fn basic() {
    // Validate default values.
    let mut vp = LegacyViewingPipeline::new();
    assert!(vp.display_view_transform().is_none());
    assert!(vp.channel_view().is_none());
    assert!(vp.color_timing_cc().is_none());
    assert!(vp.display_cc().is_none());
    assert!(vp.linear_cc().is_none());
    assert!(!vp.looks_override_enabled());
    assert_eq!(vp.looks_override(), b"");

    // An empty viewing pipeline transform is not valid.
    let mut config = (*Config::create_raw().unwrap()).clone();
    let ctx = config.current_context().get();
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "can't create a processor without a display transform",
    );

    // Validate setters.

    let mut dte = DisplayViewTransform::new();
    vp.set_display_view_transform(Some(&dte));
    assert!(vp.display_view_transform().is_some());

    // Display transform member has to be valid.
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "LegacyViewingPipeline is not valid: DisplayViewTransform: empty source color space name",
    );

    dte.set_src("colorspace1");
    vp.set_display_view_transform(Some(&dte));

    // Display transform still invalid: missing display/view.
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "LegacyViewingPipeline is not valid: DisplayViewTransform: empty display name",
    );

    dte.set_display("sRGB");
    dte.set_view("view1");
    vp.set_display_view_transform(Some(&dte));

    // Validation is fine but missing elements in config.
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "LegacyViewingPipeline error: Cannot find inputColorSpace, named 'colorspace1'",
    );

    let mut cs = ColorSpace::new();
    cs.set_name("colorspace1");
    cs.set_transform(
        Some(&ff(FixedFunctionStyle::AcesRedMod03)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    config.add_color_space(&cs).unwrap();

    config
        .add_display_view("sRGB", "view1", "colorspace1", "")
        .unwrap();

    let ctx = config.current_context().get();
    vp.processor(&config, Some(&ctx)).unwrap();

    let ff = ff(FixedFunctionStyle::AcesRedMod03);
    vp.set_channel_view(Some(&ff)).unwrap();
    assert!(vp.channel_view().is_some());
    vp.processor(&config, Some(&ctx)).unwrap();
    vp.set_channel_view(None).unwrap();
    assert!(vp.channel_view().is_none());

    vp.set_color_timing_cc(Some(&ff)).unwrap();
    assert!(vp.color_timing_cc().is_some());
    // Missing element: color_timing role.
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "ColorTimingCC requires 'color_timing' role to be defined",
    );
    vp.set_color_timing_cc(None).unwrap();
    assert!(vp.color_timing_cc().is_none());

    vp.set_linear_cc(Some(&ff)).unwrap();
    assert!(vp.linear_cc().is_some());
    // Missing element: scene_linear role.
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "LinearCC requires 'scene_linear' role to be defined",
    );
    vp.set_linear_cc(None).unwrap();
    assert!(vp.linear_cc().is_none());

    vp.set_display_cc(Some(&ff)).unwrap();
    assert!(vp.display_cc().is_some());
    vp.processor(&config, Some(&ctx)).unwrap();
    vp.set_display_cc(None).unwrap();
    assert!(vp.display_cc().is_none());

    vp.set_looks_override("missingLook");
    assert_eq!(vp.looks_override(), b"missingLook");

    // Look is missing but looks override is not enabled.
    vp.processor(&config, Some(&ctx)).unwrap();

    vp.set_looks_override_enabled(true);
    assert!(vp.looks_override_enabled());

    // Missing look error.
    check_throw_what(
        vp.processor(&config, Some(&ctx)),
        "The specified look, 'missingLook', cannot be found",
    );
}

/// Port of `OCIO_ADD_TEST(LegacyViewingPipeline, processorWithLooks)` @ v2.5.2.
#[test]
fn processor_with_looks() {
    let cfg = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    cfg.validate().unwrap();
    let ctx = cfg.current_context().get();

    let mut dt = DisplayViewTransform::new();
    dt.set_display("DISP_2");
    dt.set_view("VIEW_2");
    dt.set_src("in_1");
    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));

    let mut mat = MatrixTransform::new();
    let mut m: [f64; 16] = [
        1.1, 0., 0., 0., //
        0., 1.2, 0., 0., //
        0., 0., 1.1, 0., //
        0., 0., 0., 1.,
    ];
    mat.set_matrix(&m);
    vp.set_channel_view(Some(&Transform::from(mat.clone())))
        .unwrap();

    vp.set_linear_cc(Some(&ff(FixedFunctionStyle::AcesRedMod03)))
        .unwrap();

    // Processor in forward direction.

    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    let group_transform = group_of(&proc);
    assert_eq!(group_transform.num_transforms(), 8);
    // LinearCC creates a color space conversion and a transform.
    {
        // Color space conversion from in_1 to scene_linear role (lin_1 color space).
        let exp = typed!(group_transform.transform(0).unwrap(), Exponent);
        assert_eq!(exp.direction(), TransformDirection::Forward);
        let values = exp.value();
        assert_eq!(values[0], 2.6);
        assert_eq!(values[1], 2.6);
        assert_eq!(values[2], 2.6);
        assert_eq!(values[3], 1.);

        // LinearCC transform.
        let fft = typed!(group_transform.transform(1).unwrap(), FixedFunction);
        assert_eq!(fft.direction(), TransformDirection::Forward);
    }
    // Apply the looks, channel view, and view transform.
    {
        // Lin_1 to look3 process space (log_1).
        let log = typed!(group_transform.transform(2).unwrap(), Log);
        assert_eq!(log.direction(), TransformDirection::Forward);
        assert_eq!(log.base(), 2.);

        // Look_3 transform.
        let cdl = typed!(group_transform.transform(3).unwrap(), Cdl);
        assert_eq!(cdl.direction(), TransformDirection::Forward);
        let rgb = cdl.slope();
        assert_eq!(rgb[0], 1.);
        assert_eq!(rgb[1], 2.);
        assert_eq!(rgb[2], 1.);

        // Look_3 & look_4 have the same process space, no color space conversion.

        // Look_4 transform.
        let cdl = typed!(group_transform.transform(4).unwrap(), Cdl);
        assert_eq!(cdl.direction(), TransformDirection::Inverse);
        let rgb = cdl.slope();
        assert_eq!(rgb[0], 1.2);
        assert_eq!(rgb[1], 2.2);
        assert_eq!(rgb[2], 1.2);

        // Channel View transform (no color space conversion).
        let mattr = typed!(group_transform.transform(5).unwrap(), Matrix);
        assert_eq!(mattr.direction(), TransformDirection::Forward);
        let m2 = mattr.matrix();
        assert_eq!(m2[0], 1.1);
        assert_eq!(m2[1], 0.);
        assert_eq!(m2[2], 0.);
        assert_eq!(m2[3], 0.);
        assert_eq!(m2[5], 1.2);
        assert_eq!(m2[10], 1.1);

        // Look_4 process color space (log_1) to reference.
        let log = typed!(group_transform.transform(6).unwrap(), Log);
        assert_eq!(log.direction(), TransformDirection::Inverse);
        assert_eq!(log.base(), 2.);

        // Reference to view_2 color space.
        let exp = typed!(group_transform.transform(7).unwrap(), Exponent);
        assert_eq!(exp.direction(), TransformDirection::Inverse);
        let values = exp.value();
        assert_eq!(values[0], 2.4);
        assert_eq!(values[1], 2.4);
        assert_eq!(values[2], 2.4);
        assert_eq!(values[3], 1.);
    }

    // Repeat in inverse direction.

    dt.set_direction(TransformDirection::Inverse);
    vp.set_display_view_transform(Some(&dt));
    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    let group_transform = group_of(&proc);
    assert_eq!(group_transform.num_transforms(), 8);

    // Apply the inverse view transform, channel view, and looks.
    {
        // View_2 to reference.
        let exp = typed!(group_transform.transform(0).unwrap(), Exponent);
        assert_eq!(exp.direction(), TransformDirection::Forward);
        let values = exp.value();
        assert_eq!(values[0], 2.4);
        assert_eq!(values[1], 2.4);
        assert_eq!(values[2], 2.4);
        assert_eq!(values[3], 1.);

        // Reference to look_4 process color space (log_1).
        let log = typed!(group_transform.transform(1).unwrap(), Log);
        assert_eq!(log.direction(), TransformDirection::Forward);
        assert_eq!(log.base(), 2.);

        // Channel View transform.
        let mattr = typed!(group_transform.transform(2).unwrap(), Matrix);
        assert_eq!(mattr.direction(), TransformDirection::Forward);
        let m2 = mattr.matrix();
        assert_eq!(m2[0], 1. / 1.1);
        assert_eq!(m2[5], 1. / 1.2);
        assert_eq!(m2[10], 1. / 1.1);

        // Look_4 transform.
        let cdl = typed!(group_transform.transform(3).unwrap(), Cdl);
        assert_eq!(cdl.direction(), TransformDirection::Forward);
        let rgb = cdl.slope();
        assert_eq!(rgb[0], 1.2);
        assert_eq!(rgb[1], 2.2);
        assert_eq!(rgb[2], 1.2);

        // Look_3 transform.
        let cdl = typed!(group_transform.transform(4).unwrap(), Cdl);
        assert_eq!(cdl.direction(), TransformDirection::Inverse);
        let rgb = cdl.slope();
        assert_eq!(rgb[0], 1.);
        assert_eq!(rgb[1], 2.);
        assert_eq!(rgb[2], 1.);

        // Look_3 process color space (log_1) to lin_1.
        let log = typed!(group_transform.transform(5).unwrap(), Log);
        assert_eq!(log.direction(), TransformDirection::Inverse);
        assert_eq!(log.base(), 2.);
    }
    // LinearCC color space conversion and transform.
    {
        // LinearCC transform.
        let fft = typed!(group_transform.transform(6).unwrap(), FixedFunction);
        assert_eq!(fft.direction(), TransformDirection::Inverse);

        // LnearCC color space conversion.
        let exp = typed!(group_transform.transform(7).unwrap(), Exponent);
        assert_eq!(exp.direction(), TransformDirection::Inverse);
        let values = exp.value();
        assert_eq!(values[0], 2.6);
        assert_eq!(values[1], 2.6);
        assert_eq!(values[2], 2.6);
        assert_eq!(values[3], 1.);
    }

    // Channel view with alpha will cause color space conversions to be skipped if data bypass
    // is enabled (looks are also bypassed).

    m[3] = 0.1;
    mat.set_matrix(&m);
    vp.set_channel_view(Some(&Transform::from(mat.clone())))
        .unwrap();
    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    let group_transform = group_of(&proc);
    assert_eq!(group_transform.num_transforms(), 2);
    {
        // Channel view.
        typed!(group_transform.transform(0).unwrap(), Matrix);
        // LinearCC transform.
        typed!(group_transform.transform(1).unwrap(), FixedFunction);
    }

    // Looks are still applied if looks override is used.

    vp.set_looks_override_enabled(true);
    vp.set_looks_override(cfg.display_view_looks("DISP_2", "VIEW_2"));

    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    let group_transform = group_of(&proc);
    assert_eq!(group_transform.num_transforms(), 4);
    {
        // Channel view.
        typed!(group_transform.transform(0).unwrap(), Matrix);
        // Look_4 transform.
        typed!(group_transform.transform(1).unwrap(), Cdl);
        // Look_3 transform.
        typed!(group_transform.transform(2).unwrap(), Cdl);
        // LinearCC transform.
        typed!(group_transform.transform(3).unwrap(), FixedFunction);
    }

    dt.set_data_bypass(false);
    vp.set_display_view_transform(Some(&dt));
    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    let group_transform = group_of(&proc);
    assert_eq!(group_transform.num_transforms(), 8);
}

/// Port of `OCIO_ADD_TEST(LegacyViewingPipeline, fullPipelineNoLook)` @ v2.5.2.
#[test]
fn full_pipeline_no_look() {
    //
    // Validate BuildDisplayOps where the display/view is a simple color space (i.e., no
    // ViewTransform).
    //

    let src = "source";
    let dst = "destination";
    let linear_cs = "linear_cs";
    let timing_cs = "color_timing_cs";

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    let mut cs_source = ColorSpace::new();
    cs_source.set_name(src);
    let mut mat = MatrixTransform::new();
    let offset_src: [f64; 4] = [0., 0.1, 0.2, 0.];
    mat.set_offset(&offset_src);
    cs_source
        .set_transform(
            Some(&Transform::from(mat)),
            ColorSpaceDirection::ToReference,
        )
        .unwrap();
    cfg.add_color_space(&cs_source).unwrap();

    let mut cs = ColorSpace::new();
    cs.set_name(dst);
    cs.set_transform(
        Some(&ff(FixedFunctionStyle::AcesGlow03)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cfg.add_color_space(&cs).unwrap();

    let mut cs = ColorSpace::new();
    cs.set_name(linear_cs);
    cs.set_transform(
        Some(&ff(FixedFunctionStyle::AcesGlow10)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cs.set_transform(
        Some(&ff(FixedFunctionStyle::AcesRedMod10)),
        ColorSpaceDirection::ToReference,
    )
    .unwrap();
    cfg.add_color_space(&cs).unwrap();
    cfg.set_role(ROLE_SCENE_LINEAR, Some(linear_cs.as_bytes()))
        .unwrap();

    let mut cs = ColorSpace::new();
    cs.set_name(timing_cs);
    cs.set_transform(
        Some(&ff(FixedFunctionStyle::RgbToHsv)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cs.set_transform(
        Some(&ff(FixedFunctionStyle::AcesDarkToDim10)),
        ColorSpaceDirection::ToReference,
    )
    .unwrap();
    cfg.add_color_space(&cs).unwrap();
    cfg.set_role(ROLE_COLOR_TIMING, Some(timing_cs.as_bytes()))
        .unwrap();

    let display = "display";
    let view = "view";
    cfg.add_display_view(display, view, dst, "").unwrap();

    cfg.validate().unwrap();

    let mut dt = DisplayViewTransform::new();
    dt.set_src(src);

    dt.set_display(display);
    dt.set_view(view);

    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));

    let mut linear_cc = MatrixTransform::new();
    let offset_linear_cc: [f64; 4] = [0.2, 0.3, 0.4, 0.];
    linear_cc.set_offset(&offset_linear_cc);
    vp.set_linear_cc(Some(&Transform::from(linear_cc))).unwrap();
    let mut timimg_cc = ExponentTransform::new();
    let value_timing_cc: [f64; 4] = [2.2, 2.3, 2.4, 1.];
    timimg_cc.set_value(&value_timing_cc);
    vp.set_color_timing_cc(Some(&Transform::from(timimg_cc)))
        .unwrap();
    let offset_cv: [f64; 4] = [0.2, 0.1, 0.1, 0.];
    let mut cv_trans = MatrixTransform::new();
    cv_trans.set_offset(&offset_cv);
    vp.set_channel_view(Some(&Transform::from(cv_trans)))
        .unwrap();
    let display_cc = ExposureContrastTransform::new();
    vp.set_display_cc(Some(&Transform::from(display_cc)))
        .unwrap();

    {
        let ctx = cfg.current_context().get();
        let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
        let group_transform = group_of(&proc);
        assert_eq!(group_transform.num_transforms(), 10);

        // 0-1. InputCS -> scene linear role:
        //     0. Input to reference.
        //     1. Scene linear role from reference.
        // 2. LinearCC.
        // 3-4. Scene linear -> color timing role:
        //     4. Scene linear role to reference.
        //     5. ColorTiming from reference.
        // 5. ColorTimingCC.
        // * No look.
        // 6. ChannelView.
        // 7-8. Color timing role -> display/view color space:
        //     7. ColorTiming to reference.
        //     8. DisplayCS from reference.
        // 9. DisplayCC.

        // 0. Input to reference.
        let mat = typed!(group_transform.transform(0).unwrap(), Matrix);
        assert_eq!(mat.direction(), TransformDirection::Forward);
        let offset = mat.offset();
        assert_eq!(offset, offset_src);

        // 1. Scene linear role from reference.
        let ff = typed!(group_transform.transform(1).unwrap(), FixedFunction);
        assert_eq!(ff.direction(), TransformDirection::Forward);
        assert_eq!(ff.style(), FixedFunctionStyle::AcesGlow10);

        // 2. LinearCC.
        let mat = typed!(group_transform.transform(2).unwrap(), Matrix);
        assert_eq!(mat.direction(), TransformDirection::Forward);
        assert_eq!(mat.offset(), offset_linear_cc);

        // 3. Scene linear role to reference.
        let ff = typed!(group_transform.transform(3).unwrap(), FixedFunction);
        assert_eq!(ff.direction(), TransformDirection::Forward);
        assert_eq!(ff.style(), FixedFunctionStyle::AcesRedMod10);

        // 4. ColorTiming from reference.
        let ff = typed!(group_transform.transform(4).unwrap(), FixedFunction);
        assert_eq!(ff.direction(), TransformDirection::Forward);
        assert_eq!(ff.style(), FixedFunctionStyle::RgbToHsv);

        // 5. ColorTimingCC.
        let exp = typed!(group_transform.transform(5).unwrap(), Exponent);
        assert_eq!(exp.direction(), TransformDirection::Forward);
        assert_eq!(exp.value(), value_timing_cc);

        // 6. ChannelView.
        let mat = typed!(group_transform.transform(6).unwrap(), Matrix);
        assert_eq!(mat.direction(), TransformDirection::Forward);
        assert_eq!(mat.offset(), offset_cv);

        // 7. ColorTiming to reference.
        let ff = typed!(group_transform.transform(7).unwrap(), FixedFunction);
        assert_eq!(ff.direction(), TransformDirection::Forward);
        assert_eq!(ff.style(), FixedFunctionStyle::AcesDarkToDim10);

        // 8. DisplayCS from reference.
        let ff = typed!(group_transform.transform(8).unwrap(), FixedFunction);
        assert_eq!(ff.direction(), TransformDirection::Forward);
        assert_eq!(ff.style(), FixedFunctionStyle::AcesGlow03);

        // 9. DisplayCC.
        typed!(group_transform.transform(9).unwrap(), ExposureContrast);
    }

    //
    // Using a scene-referred ViewTransform.
    //

    let dsp = "display";
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs.set_name(dsp);
    let ec = ExposureContrastTransform::new();
    cs.set_transform(
        Some(&Transform::from(ec)),
        ColorSpaceDirection::FromReference,
    )
    .unwrap();
    cfg.add_color_space(&cs).unwrap();

    let scenevt = "scene_vt";
    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name(scenevt);
    let mut log = LogTransform::new();
    log.set_base(4.2);
    vt.set_transform(
        Some(&Transform::from(log)),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    cfg.add_view_transform(&vt).unwrap();

    let viewt = "viewt";
    cfg.add_display_view_with_view_transform(display, viewt, scenevt, dsp, "", "", "")
        .unwrap();
    cfg.validate().unwrap();

    dt.set_view(viewt);
    vp.set_display_view_transform(Some(&dt));

    {
        let ctx = cfg.current_context().get();
        let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
        let group_transform = group_of(&proc);
        // Getting an additional op for the reference space change.
        assert_eq!(group_transform.num_transforms(), 11);

        // Same as previous up to colorTiming to reference.
        // 0. Input to reference.
        // 1. Scene linear role from reference.
        // 2. LinearCC.
        // 3. Scene linear role to reference.
        // 4. ColorTiming from reference.
        // 5. ColorTimingCC.
        // 6. ChannelView.
        // 7. ColorTiming to reference.

        // 8. Changing from scene-referred space to display-referred space done with the
        //    specified view transform.
        let log = typed!(group_transform.transform(8).unwrap(), Log);
        assert_eq!(log.direction(), TransformDirection::Forward);
        assert_eq!(log.base(), 4.2);

        // 9. DisplayCS from reference.
        typed!(group_transform.transform(9).unwrap(), ExposureContrast);

        // 10. DisplayCC.
        typed!(group_transform.transform(10).unwrap(), ExposureContrast);
    }

    //
    // Adding a display-referred ViewTransform.
    //

    let displayvt = "display_vt";
    let mut vt = ViewTransform::new(ReferenceSpaceType::Display);
    vt.set_name(displayvt);
    let mut log = LogTransform::new();
    log.set_base(2.1);
    vt.set_transform(
        Some(&Transform::from(log)),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    cfg.add_view_transform(&vt).unwrap();

    // Replace view display.
    cfg.add_display_view_with_view_transform(display, viewt, displayvt, dsp, "", "", "")
        .unwrap();
    cfg.validate().unwrap();

    {
        let ctx = cfg.current_context().get();
        let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
        let group_transform = group_of(&proc);
        // Getting an additional op for the display to display view transform.
        assert_eq!(group_transform.num_transforms(), 12);

        // Same as previous up to scene-referred to display referred.
        // 0. Input to reference.
        // 1. Scene linear role from reference.
        // 2. LinearCC.
        // 3. Scene linear role to reference.
        // 4. ColorTiming from reference.
        // 5. ColorTimingCC.
        // 6. ChannelView.
        // 7. ColorTiming to reference.
        // 8. Changing from scene-referred space to display-referred space using the default
        //    view transform.

        // 9. Display-referred reference to display-referred reference using the specified
        //    view transform.
        let log = typed!(group_transform.transform(9).unwrap(), Log);
        assert_eq!(log.direction(), TransformDirection::Forward);
        assert_eq!(log.base(), 2.1);

        // 10. DisplayCS from reference.
        typed!(group_transform.transform(10).unwrap(), ExposureContrast);

        // 11. DisplayCC.
        typed!(group_transform.transform(11).unwrap(), ExposureContrast);
    }

    // Using a named transform.
    let mut nt = NamedTransform::new();
    nt.set_name("nt1");
    let mut nt_trans = MatrixTransform::new();
    let offset_nt: [f64; 4] = [0.01, 0.05, 0.1, 0.];
    nt_trans.set_offset(&offset_nt);
    nt.set_transform(
        Some(&Transform::from(nt_trans)),
        TransformDirection::Forward,
    )
    .unwrap();
    cfg.add_named_transform(&nt).unwrap();

    {
        let viewnt = "viewnt";
        cfg.add_display_view(display, viewnt, "nt1", "").unwrap();
        cfg.validate().unwrap();

        dt.set_view(viewnt);
        vp.set_display_view_transform(Some(&dt));

        let ctx = cfg.current_context().get();
        let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
        let group_transform = group_of(&proc);
        assert_eq!(group_transform.num_transforms(), 5);

        // 0. LinearCC.
        // 1. ColorTimingCC.
        // 2. ChannelView.
        // 3. Named transform.
        // 4. DisplayCC.

        typed!(group_transform.transform(0).unwrap(), Matrix);
        typed!(group_transform.transform(1).unwrap(), Exponent);
        typed!(group_transform.transform(2).unwrap(), Matrix);
        let mat = typed!(group_transform.transform(3).unwrap(), Matrix);
        assert_eq!(mat.direction(), TransformDirection::Forward);
        assert_eq!(mat.offset(), offset_nt);
        typed!(group_transform.transform(4).unwrap(), ExposureContrast);
    }

    dt.set_view(viewt);
    vp.set_display_view_transform(Some(&dt));
    cs_source.set_is_data(true);
    cfg.add_color_space(&cs_source).unwrap();
    cfg.validate().unwrap();

    {
        let ctx = cfg.current_context().get();
        let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
        let group_transform = group_of(&proc);
        // Color space conversion is skipped.
        assert_eq!(group_transform.num_transforms(), 4);

        // With isData true, the view/display transform is not applied. The CC and channelView
        // are applied, but without converting to their usual process spaces.
        // 0. LinearCC.
        // 1. ColorTimingCC.
        // 2. ChannelView.
        // 3. DisplayCC.

        typed!(group_transform.transform(0).unwrap(), Matrix);
        typed!(group_transform.transform(1).unwrap(), Exponent);
        typed!(group_transform.transform(2).unwrap(), Matrix);
        typed!(group_transform.transform(3).unwrap(), ExposureContrast);
    }
}

/// Port of `OCIO_ADD_TEST(LegacyViewingPipeline, processorWithNoOpLook)` @ v2.5.2.
#[test]
fn processor_with_no_op_look() {
    //
    // Validate LegacyViewingPipeline::getProcessor when a noop look override is specified.
    //

    let cfg = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    cfg.validate().unwrap();
    let ctx = cfg.current_context().get();

    let mut dt = DisplayViewTransform::new();
    dt.set_display("DISP_2");
    dt.set_view("VIEW_2");
    dt.set_src("in_1");

    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));
    vp.set_looks_override_enabled(true);
    vp.set_looks_override("look_noop");

    // Processor in forward direction.

    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    group_of(&proc);

    // Repeat in inverse direction.

    dt.set_direction(TransformDirection::Inverse);
    vp.set_display_view_transform(Some(&dt));
    vp.set_looks_override_enabled(true);
    vp.set_looks_override("look_noop");

    // Processor in inverse direction.

    let proc = vp.processor(&cfg, Some(&ctx)).unwrap();
    group_of(&proc);
}

/// `set_transforms_as_is` (the Python binding's hook) holds the transforms as they are: the
/// transforms the setters hold give the same pipeline (text and processor) as the setters; a
/// display view transform whose looks aren't bypassed stays so, with the bypass the setter
/// remembered kept; and none at all is the missing display transform. (The binding's oracle
/// tests compare what Python sees with the wheel's.)
#[test]
fn transforms_held_as_they_are() {
    let cfg = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    cfg.validate().unwrap();

    let mut dt = DisplayViewTransform::new();
    dt.set_display("DISP_2");
    dt.set_view("VIEW_2");
    dt.set_src("in_1");
    let mut mat = MatrixTransform::new();
    mat.set_offset(&[0.1, 0.2, 0.3, 0.]);

    let mut vp = LegacyViewingPipeline::new();
    vp.set_display_view_transform(Some(&dt));
    vp.set_linear_cc(Some(&ff(FixedFunctionStyle::AcesRedMod03)))
        .unwrap();
    vp.set_channel_view(Some(&Transform::from(mat.clone())))
        .unwrap();
    let text = vp.to_bytes();
    let id = vp.processor(&cfg, None).unwrap().cache_id().unwrap();

    let mut held = dt.clone();
    held.set_looks_bypass(true);
    let mut as_is = LegacyViewingPipeline::new();
    as_is.set_display_view_transform(Some(&dt));
    as_is.set_transforms_as_is(
        Some(held),
        Some(ff(FixedFunctionStyle::AcesRedMod03)),
        None,
        Some(Transform::from(mat.clone())),
        None,
    );
    assert_eq!(as_is.to_bytes(), text);
    assert_eq!(as_is.processor(&cfg, None).unwrap().cache_id().unwrap(), id);

    as_is.set_transforms_as_is(
        Some(dt.clone()),
        Some(ff(FixedFunctionStyle::AcesRedMod03)),
        None,
        Some(Transform::from(mat)),
        None,
    );
    assert!(!as_is.display_view_transform().unwrap().looks_bypass());
    assert_ne!(as_is.to_bytes(), text);

    as_is.set_transforms_as_is(None, None, None, None, None);
    assert!(as_is.display_view_transform().is_none() && as_is.linear_cc().is_none());
    check_throw_what(
        as_is.processor(&cfg, None),
        "LegacyViewingPipeline: can't create a processor without a display transform.",
    );
}
