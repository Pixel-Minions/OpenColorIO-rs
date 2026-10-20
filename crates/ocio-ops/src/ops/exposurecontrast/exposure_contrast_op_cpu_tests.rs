// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/exposurecontrast/ExposureContrastOpCPU_tests.cpp @ v2.5.2.

use super::*;
use crate::bit_depth_utils::get_bit_depth_max_value;
use crate::open_color_types::BitDepth;
use ocio_testkit::upstream::check_close;

/// Port of `videoECVal` (ExposureContrastOpCPU_tests.cpp:14-33 @ v2.5.2).
fn video_ec_val(input: f32, ec: &ExposureContrastOpData, in_max: f32, out_max: f32) -> f32 {
    let exposure = std_powf(2.0, ec.get_exposure() as f32).powf(ec::VIDEO_OETF_POWER as f32);
    let contrast = std_max(ec::MIN_CONTRAST, ec.get_contrast() * ec.get_gamma()) as f32;
    let pivot = (std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32).powf(ec::VIDEO_OETF_POWER as f32);

    if contrast == 1.0 {
        return input * exposure / (pivot * in_max) * pivot * out_max;
    }

    std_max(0.0f32, input * exposure / (pivot * in_max)).powf(contrast) * pivot * out_max
}

/// Port of `logECVal` (ExposureContrastOpCPU_tests.cpp:35-51 @ v2.5.2).
fn log_ec_val(input: f32, ec: &ExposureContrastOpData, in_max: f32, out_max: f32) -> f32 {
    let exposure = (ec.get_log_exposure_step() * ec.get_exposure()) as f32 * in_max;
    let contrast =
        std_max(ec::MIN_CONTRAST, ec.get_contrast() * ec.get_gamma()) as f32 * out_max / in_max;

    let pivot = std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32;
    let log_pivot = std_max(
        0.,
        (f64::from(pivot) / 0.18).log2() * ec.get_log_exposure_step() + ec.get_log_mid_gray(),
    ) as f32;
    let offset = (exposure - (log_pivot * in_max)) * contrast + (log_pivot * out_max);

    (input * contrast) + offset
}

/// Port of `linECVal` (ExposureContrastOpCPU_tests.cpp:53-68 @ v2.5.2).
fn lin_ec_val(input: f32, ec: &ExposureContrastOpData, in_max: f32, out_max: f32) -> f32 {
    let exposure = std_powf(2., ec.get_exposure() as f32);
    let contrast = std_max(ec::MIN_CONTRAST, ec.get_contrast() * ec.get_gamma()) as f32;
    let pivot = std_max(ec::MIN_PIVOT, ec.get_pivot()) as f32;

    if contrast == 1.0 {
        return input * exposure / (pivot * in_max) * pivot * out_max;
    }

    std_max(0.0f32, input * exposure / (pivot * in_max)).powf(contrast) * pivot * out_max
}

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

/// The renderer's class, as `DynamicPtrCast<...>(renderer)` checks it.
fn class_of(renderer: &Arc<dyn CpuOp>) -> String {
    let name = format!("{renderer:?}");
    name.split([' ', '{']).next().unwrap_or("").to_string()
}

/// The three dynamic properties of a renderer, as doubles: contrast, exposure, gamma.
fn renderer_properties(
    renderer: &Arc<dyn CpuOp>,
) -> (
    DynamicPropertyDoubleImplRcPtr,
    DynamicPropertyDoubleImplRcPtr,
    DynamicPropertyDoubleImplRcPtr,
) {
    let get = |t| {
        renderer
            .get_dynamic_property(t)
            .unwrap()
            .as_double()
            .unwrap()
    };
    (
        get(DynamicPropertyType::Contrast),
        get(DynamicPropertyType::Exposure),
        get(DynamicPropertyType::Gamma),
    )
}

/// An op data of `style` with its three properties dynamic.
fn dynamic_ec(style: ExposureContrastOpStyle) -> ExposureContrastOpData {
    let ec = ExposureContrastOpData::new(style);
    ec.get_exposure_property().make_dynamic();
    ec.get_contrast_property().make_dynamic();
    ec.get_gamma_property().make_dynamic();
    ec
}

/// Port of `OCIO_ADD_TEST(ExposureContrastRenderer, video)` @ v2.5.2.
#[test]
fn video() {
    //
    // Video case, no scaling
    //
    let rgba_image: Vec<f32> = vec![
        0.0367126, 0.5, 1., 0., //
        0.2, 0., 0.99, 128., //
        QNAN, QNAN, QNAN, 0., //
        INF, INF, INF, 0.,
    ];

    let ec = dynamic_ec(ExposureContrastOpStyle::Video);

    let renderer = get_exposure_contrast_cpu_renderer(&ec);
    assert_eq!(class_of(&renderer), "EcVideoRenderer");
    let mut rgba = rgba_image.clone();

    renderer.apply(&mut rgba);

    let val = |i: usize| video_ec_val(rgba_image[i], &ec, 1., 1.);
    assert_eq!(rgba[0], val(0));
    assert_eq!(rgba[1], val(1));
    assert_eq!(rgba[2], val(2));
    assert_eq!(rgba[3], rgba_image[3]);
    assert_eq!(rgba[4], val(4));
    assert_eq!(rgba[5], val(5));
    assert_eq!(rgba[6], val(6));
    assert_eq!(rgba[7], rgba_image[7]);

    assert!(rgba[8].is_nan());
    assert!(rgba[9].is_nan());
    assert!(rgba[10].is_nan());
    assert_eq!(rgba[12], val(12));
    assert_eq!(rgba[13], val(13));
    assert_eq!(rgba[14], val(14));

    // Re-test with different E/C values.
    //

    // The renderer has a copy of ec, get the dynamic property ptr in order to change the value
    // for the apply.
    let (dpc, dpe, dpg) = renderer_properties(&renderer);
    dpe.set_value(0.2);
    dpc.set_value(1.0);
    // TODO: When base < 1, ssePower(Inf, base) != Inf.  It returns various
    // large numbers, e.g., 3.6e11 for base=0.3 ranging up to 4.0e36 for base=0.95.
    // dpc->setContrast(0.5);
    dpg.set_value(1.2);

    let mut rgba = rgba_image.clone();
    renderer.apply(&mut rgba);

    // The dynamic property ptr only updates the copy in the renderer.  Need to also update the
    // original opData in this case since it is used to check the result of the apply.
    ec.set_exposure(0.2);
    ec.set_contrast(1.0);
    ec.set_gamma(1.2);

    // As the ssePower is an approximation, strict equality is not possible.
    let error = 1e-5f32;
    let val = |i: usize| video_ec_val(rgba_image[i], &ec, 1., 1.);
    check_close(rgba[0], val(0), error);
    check_close(rgba[1], val(1), error);
    check_close(rgba[2], val(2), error);
    assert_eq!(rgba[3], rgba_image[3]);
    check_close(rgba[4], val(4), error);
    check_close(rgba[5], val(5), error);
    check_close(rgba[6], val(6), error);
    assert_eq!(rgba[7], rgba_image[7]);

    check_close(rgba[8], val(8), error);
    check_close(rgba[9], val(9), error);
    check_close(rgba[10], val(10), error);
    assert_eq!(rgba[12], val(12));
    assert_eq!(rgba[13], val(13));
    assert_eq!(rgba[14], val(14));
}

/// Port of `OCIO_ADD_TEST(ExposureContrastRenderer, log)` @ v2.5.2.
#[test]
fn log() {
    //
    // Log case, no scaling
    //
    let rgba_image: Vec<f32> = vec![
        0.0367126, 0.5, 1., 0., //
        0.2, 0., 0.99, 128., //
        QNAN, QNAN, QNAN, 0., //
        INF, INF, INF, 0.,
    ];
    let ec = dynamic_ec(ExposureContrastOpStyle::Logarithmic);
    let mut ec = ec;
    ec.set_exposure(1.2);
    ec.set_pivot(0.18);

    let renderer = get_exposure_contrast_cpu_renderer(&ec);
    assert_eq!(class_of(&renderer), "EcLogarithmicRenderer");

    let mut rgba = rgba_image.clone();

    renderer.apply(&mut rgba);

    let in_max = get_bit_depth_max_value(BitDepth::F32).unwrap() as f32;
    let out_max = get_bit_depth_max_value(BitDepth::F32).unwrap() as f32;

    let check = |rgba: &[f32], ec: &ExposureContrastOpData| {
        let val = |i: usize| log_ec_val(rgba_image[i], ec, in_max, out_max);
        assert_eq!(rgba[0], val(0));
        assert_eq!(rgba[1], val(1));
        assert_eq!(rgba[2], val(2));
        assert_eq!(rgba[3], rgba_image[3] * out_max / in_max);
        assert_eq!(rgba[4], val(4));
        assert_eq!(rgba[5], val(5));
        assert_eq!(rgba[6], val(6));
        assert_eq!(rgba[7], rgba_image[7] * out_max / in_max);

        assert!(rgba[8].is_nan());
        assert!(rgba[9].is_nan());
        assert!(rgba[10].is_nan());
        let val = |i: usize| log_ec_val(rgba_image[i], ec, 1., 1.);
        assert_eq!(rgba[12], val(12));
        assert_eq!(rgba[13], val(13));
        assert_eq!(rgba[14], val(14));
    };
    check(&rgba, &ec);

    // Re-test with different E/C values.
    //

    let (dpc, dpe, dpg) = renderer_properties(&renderer);
    dpe.set_value(0.2);
    dpc.set_value(0.5);
    dpg.set_value(1.6);

    let mut rgba = rgba_image.clone();
    renderer.apply(&mut rgba);

    ec.set_exposure(0.2);
    ec.set_contrast(0.5);
    ec.set_gamma(1.6);

    check(&rgba, &ec);
}

/// Port of `OCIO_ADD_TEST(ExposureContrastRenderer, linear)` @ v2.5.2.
#[test]
fn linear() {
    //
    // Linear case, no scaling
    //
    let rgba_image: Vec<f32> = vec![
        0.0, 0.5, 1., 0., //
        0.2, 0.8, 0.99, 128., //
        QNAN, QNAN, QNAN, 0., //
        INF, INF, INF, 0.,
    ];
    let ec = dynamic_ec(ExposureContrastOpStyle::Linear);

    let renderer = get_exposure_contrast_cpu_renderer(&ec);
    assert_eq!(class_of(&renderer), "EcLinearRenderer");

    let mut rgba = rgba_image.clone();

    renderer.apply(&mut rgba);

    let val = |i: usize| lin_ec_val(rgba_image[i], &ec, 1., 1.);
    assert_eq!(rgba[0], val(0));
    assert_eq!(rgba[1], val(1));
    assert_eq!(rgba[2], val(2));
    assert_eq!(rgba[3], rgba_image[3]);
    assert_eq!(rgba[4], val(4));
    assert_eq!(rgba[5], val(5));
    assert_eq!(rgba[6], val(6));
    assert_eq!(rgba[7], rgba_image[7]);

    assert!(rgba[8].is_nan());
    assert!(rgba[9].is_nan());
    assert!(rgba[10].is_nan());
    assert_eq!(rgba[12], val(12));
    assert_eq!(rgba[13], val(13));
    assert_eq!(rgba[14], val(14));

    // Re-test with different E/C values.
    //

    let (dpc, dpe, dpg) = renderer_properties(&renderer);
    dpe.set_value(0.2);
    dpc.set_value(1.5);
    dpg.set_value(1.2);

    let mut rgba = rgba_image.clone();
    renderer.apply(&mut rgba);

    ec.set_exposure(0.2);
    ec.set_contrast(1.5);
    ec.set_gamma(1.2);

    // As the ssePower is an approximation, strict equality is not possible.
    let error = 5e-5f32;

    let val = |i: usize| lin_ec_val(rgba_image[i], &ec, 1., 1.);
    check_close(rgba[0], val(0), error);
    check_close(rgba[1], val(1), error);
    check_close(rgba[2], val(2), error);
    assert_eq!(rgba[3], rgba_image[3]);
    check_close(rgba[4], val(4), error);
    check_close(rgba[5], val(5), error);
    check_close(rgba[6], val(6), error);
    assert_eq!(rgba[7], rgba_image[7]);

    check_close(rgba[8], val(8), error);
    check_close(rgba[9], val(9), error);
    check_close(rgba[10], val(10), error);
    assert_eq!(rgba[12], val(12));
    assert_eq!(rgba[13], val(13));
    assert_eq!(rgba[14], val(14));
}

/// Port of `TestECInverse` (ExposureContrastOpCPU_tests.cpp:338-370 @ v2.5.2).
fn test_ec_inverse(style: ExposureContrastOpStyle) {
    let rgba_image: Vec<f32> = vec![0.0, 0.5, 1., 0., 0.2, 0.8, 0.99, 128.];

    let mut ec = ExposureContrastOpData::new(style);

    ec.set_exposure(1.5);
    ec.set_contrast(0.5);
    ec.set_gamma(1.1);
    ec.set_pivot(0.18);

    let renderer = get_exposure_contrast_cpu_renderer(&ec);

    let mut rgba = rgba_image.clone();
    renderer.apply(&mut rgba);

    let eci = ec.inverse();
    let rendereri = get_exposure_contrast_cpu_renderer(&eci);
    rendereri.apply(&mut rgba);

    // As the ssePower is an approximation, strict equality is not possible.
    let error = 1e-5f32;

    check_close(rgba[0], rgba_image[0], error);
    check_close(rgba[1], rgba_image[1], error);
    check_close(rgba[2], rgba_image[2], error);
    assert_eq!(rgba[3], rgba_image[3]);
    check_close(rgba[4], rgba_image[4], error);
    check_close(rgba[5], rgba_image[5], error);
    check_close(rgba[6], rgba_image[6], error);
    assert_eq!(rgba[7], rgba_image[7]);
}

/// Port of `OCIO_ADD_TEST(ExposureContrastRenderer, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    test_ec_inverse(ExposureContrastOpStyle::Logarithmic);
    test_ec_inverse(ExposureContrastOpStyle::Linear);
    test_ec_inverse(ExposureContrastOpStyle::Video);
}

/// Port of `TestLogParamForStyle` (ExposureContrastOpCPU_tests.cpp:379-421 @ v2.5.2).
fn test_log_param_for_style(style: ExposureContrastOpStyle, has_effect: bool) {
    let rgba_image: Vec<f32> = vec![
        0.1, 0.2, 0.3, 0., //
        0.4, 0.5, 0.6, 0., //
        0.7, 0.8, 0.9, 0.,
    ];

    let mut ec = ExposureContrastOpData::new(style);

    ec.set_exposure(0.2);
    ec.set_contrast(1.0);
    ec.set_gamma(1.2);

    let mut rgba_ref = rgba_image.clone();

    // Reference.
    {
        let renderer = get_exposure_contrast_cpu_renderer(&ec);
        renderer.apply(&mut rgba_ref);
    }

    // Change log parameters.
    ec.set_log_exposure_step(0.1);
    ec.set_log_mid_gray(0.4);
    let mut rgba = rgba_image.clone();
    {
        let renderer = get_exposure_contrast_cpu_renderer(&ec);
        renderer.apply(&mut rgba);
        for i in 0..12 {
            if !has_effect || i % 4 == 3 {
                // EC does not affect alpha
                assert_eq!(rgba[i], rgba_ref[i], "{style:?} {i}");
            } else {
                assert_ne!(rgba[i], rgba_ref[i], "{style:?} {i}");
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(ExposureContrastRenderer, log_params)` @ v2.5.2.
#[test]
fn log_params() {
    test_log_param_for_style(ExposureContrastOpStyle::Video, false);
    test_log_param_for_style(ExposureContrastOpStyle::VideoRev, false);
    test_log_param_for_style(ExposureContrastOpStyle::Linear, false);
    test_log_param_for_style(ExposureContrastOpStyle::LinearRev, false);
    test_log_param_for_style(ExposureContrastOpStyle::Logarithmic, true);
    test_log_param_for_style(ExposureContrastOpStyle::LogarithmicRev, true);
}
