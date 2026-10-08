// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the built-in transform: `tests/cpu/transforms/BuiltinTransform_tests.cpp` @ v2.5.2.
//! `color_matrix_helpers` comes with the helpers (WP 3.2e); `forward_inverse` with the
//! inverse 1D LUT (`p2-api-parity`); `interpolate` with `Interpolate1D`
//! (`builtins/op_helpers_tests.rs`); `validate` and the ACES 2.0 tests with the built-ins' ops
//! (`p3-after-p2`). The text and the validation are compared with the wheel's in
//! `tests/config_transforms_oracle.rs`.

use super::*;
use crate::transform::Transform;
use ocio_testkit::upstream::{check_throw_what, equal_with_safe_rel_error};

/// Port of `OCIO_ADD_TEST(BuiltinTransform, creation)` @ v2.5.2.
#[test]
fn creation() {
    // Tests around the creation of a built-in transform instance.

    let mut blt = BuiltinTransform::new();

    assert_eq!(blt.direction(), TransformDirection::Forward);
    assert_eq!(blt.style(), b"IDENTITY");
    assert!(blt.validate().is_ok());

    assert!(
        blt.set_style("UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD")
            .is_ok()
    );
    assert_eq!(blt.style(), b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
    assert!(blt.validate().is_ok());

    assert_eq!(
        &b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"[..],
        blt.description()
    );

    blt.set_direction(TransformDirection::Inverse);
    assert_eq!(blt.direction(), TransformDirection::Inverse);
    assert!(blt.validate().is_ok());

    // The style is case insensitive.
    assert!(
        blt.set_style("UTILITY - ACES-AP0_to_cie-xyz-D65_BFD")
            .is_ok()
    );
    assert!(blt.validate().is_ok());

    // Try an unknown style.
    check_throw_what(
        blt.set_style("UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD_UNKNOWN"),
        "BuiltinTransform: invalid built-in transform style \
         'UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD_UNKNOWN'.",
    );
}

/// Port of `OCIO_ADD_TEST(BuiltinTransform, access)` @ v2.5.2.
#[test]
fn access() {
    // Only test some default built-in transforms.

    assert_eq!(
        &b"IDENTITY"[..],
        BuiltinTransformRegistry::get().builtin_style(0).unwrap()
    );

    assert_eq!(
        &b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD"[..],
        BuiltinTransformRegistry::get().builtin_style(1).unwrap()
    );

    assert_eq!(
        &b"Convert ACES AP0 primaries to CIE XYZ with a D65 white point with Bradford adaptation"[..],
        BuiltinTransformRegistry::get()
            .builtin_description(1)
            .unwrap()
    );
}

/// A style is read up to its first NUL; an unknown one leaves the transform as it was.
#[test]
fn set_style_stops_at_nul_and_keeps_the_style_on_error() {
    let mut blt = BuiltinTransform::new();
    blt.set_style(b"utility - aces-ap0_to_cie-xyz-d65_bfd\0junk")
        .unwrap();
    assert_eq!(blt.style(), b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
    check_throw_what(
        blt.set_style(b"\0IDENTITY"),
        "BuiltinTransform: invalid built-in transform style ''.",
    );
    assert_eq!(blt.style(), b"UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD");
}

/// Port of `OCIO_ADD_TEST(BuiltinTransform, forward_inverse)` @ v2.5.2.
#[test]
fn forward_inverse() {
    use crate::config::Config;
    use crate::transform::Transform;
    use crate::transforms::group_transform::GroupTransform;
    use ocio_ops::open_color_types::{BitDepth, OptimizationFlags};

    // A forward and inverse built-in transform must be optimized out.

    // Note: As the optimization is performed using the Ops (i.e. resulting from the built-in
    // transforms), it depends on the op list optimizations and not on the transform list.

    let mut fwd_builtin = BuiltinTransform::new();
    fwd_builtin.set_style("ACEScct_to_ACES2065-1").unwrap();
    fwd_builtin.set_direction(TransformDirection::Forward);
    fwd_builtin.validate().unwrap();

    let mut inv_builtin = BuiltinTransform::new();
    inv_builtin.set_style("ACEScct_to_ACES2065-1").unwrap();
    inv_builtin.set_direction(TransformDirection::Inverse);
    inv_builtin.validate().unwrap();

    let mut grp = GroupTransform::new();
    grp.append_transform(Transform::from(fwd_builtin));
    grp.append_transform(Transform::from(inv_builtin));
    // Content is [BuiltinTransform, BuiltinTransform].
    assert_eq!(grp.num_transforms(), 2);

    let config = Config::create_raw().unwrap();
    let mut proc = config.processor(&Transform::from(grp)).unwrap();

    // Without any optimizations.
    {
        proc = proc
            .optimized_processor_with_bit_depths(
                BitDepth::F32,
                BitDepth::F32,
                OptimizationFlags::NONE,
            )
            .unwrap();

        let grp = proc.create_group_transform().unwrap();
        // Content is [Lut1DTransform, MatrixTransform, MatrixTransform, Lut1DTransform].
        assert_eq!(4, grp.num_transforms());
    }

    // With default optimizations.
    {
        proc = proc
            .optimized_processor_with_bit_depths(
                BitDepth::F32,
                BitDepth::F32,
                OptimizationFlags::DEFAULT,
            )
            .unwrap();

        let grp = proc.create_group_transform().unwrap();
        // All transforms have been optimized out.
        assert_eq!(0, grp.num_transforms());
    }
}

/// Port of `ValidateValues(const char *, T act, T aim, T errorThreshold, int lineNo)`
/// (BuiltinTransform_tests.cpp:120-145 @ v2.5.2): a relative error, absolute below 1, within
/// `error_threshold`.
#[track_caller]
fn validate_values(prefix_msg: &str, act: f32, aim: f32, error_threshold: f32) {
    // Using rel error with a large minExpected value of 1 will transition
    // from absolute error for expected values < 1 and
    // relative error for values > 1.
    assert!(
        equal_with_safe_rel_error(act, aim, error_threshold, 1.0),
        "{prefix_msg}:  - Values: {act} expected: {aim} (threshold: {error_threshold})"
    );
}

/// The processor of `transform` in a raw config.
fn raw_processor(transform: Transform) -> std::sync::Arc<crate::processor::Processor> {
    use crate::config::Config;

    let config = Config::create_raw().unwrap();
    config.processor(&transform).unwrap()
}

/// Port of `ValidateBuiltinTransform` (BuiltinTransform_tests.cpp:335-367 @ v2.5.2).
#[track_caller]
fn validate_builtin_transform(style: &str, in_: &[f32], out: &[f32], error_threshold: f32) {
    use ocio_ops::image_desc::PackedImageDesc;
    use ocio_ops::open_color_types::OptimizationFlags;

    let mut builtin = BuiltinTransform::new();
    builtin.set_style(style).unwrap();
    builtin.set_direction(TransformDirection::Forward);
    builtin.validate().unwrap();

    let proc = raw_processor(Transform::from(builtin));

    // Use lossless mode for these tests (e.g. FAST_LOG_EXP_POW limits to about 4 sig. digits).
    let cpu = proc
        .optimized_cpu_processor(OptimizationFlags::LOSSLESS)
        .unwrap();

    let mut results = vec![-1.0f32; in_.len()];
    {
        let in_desc = PackedImageDesc::new(in_, in_.len() / 3, 1, 3).unwrap();
        let mut out_desc = PackedImageDesc::new(&mut results[..], in_.len() / 3, 1, 3).unwrap();

        cpu.apply_src_dst(&in_desc, &mut out_desc).unwrap();
    }

    for idx in 0..out.len() {
        let msg = format!("{style}: for index = {idx}");
        validate_values(&msg, results[idx], out[idx], error_threshold);
    }
}

/// `UnitTestValues` (BuiltinTransform_tests.cpp:369-724 @ v2.5.2): each built-in's name, error
/// threshold, input values and expected output values.
#[rustfmt::skip]
const UNIT_TEST_VALUES: &[(&str, f32, &[f32], &[f32])] = &[
    // Contains the name, the input values and the expected output values.
    (
        "IDENTITY",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.5, 0.4, 0.3],
    ),
    (
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.472347603390, 0.440425934827, 0.326581044758],
    ),
    (
        "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.428407900093, 0.420968434905, 0.325777868096],
    ),
    (
        "UTILITY - ACES-AP1_to_LINEAR-REC709_BFD",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.578830986466, 0.388029190156, 0.282302431033],
    ),
    (
        "CURVE - ACEScct-LOG_to_LINEAR",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.514056913328, 0.152618314084, 0.045310838527],
    ),
    (
        "ACEScct_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.386397222658, 0.158557251811, 0.043152537925],
    ),
    (
        "ACEScc_to_ACES2065-1",
        // { { 0.5f, 0.4f, 0.3f }, { 0.386397222658f, 0.158557251811f, 0.043152537925f } } },
        // TODO: Hacked the red value as it is not quite within tolerance.
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.386398554, 0.158557251811, 0.043152537925],
    ),
    (
        "ACEScg_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.453158317919, 0.394926024520, 0.299297344519],
    ),
    (
        "ACESproxy10i_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.433437174444, 0.151629880817, 0.031769555400],
    ),
    (
        "ADX10_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.210518101020, 0.148655364394, 0.085189053481],
    ),
    (
        "ADX16_to_ACES2065-1",
        1.0e-6,
        &[0.125, 0.1, 0.075],
        &[0.211320835792, 0.149169650771, 0.085452970479],
    ),
    (
        "ACES-LMT - BLUE_LIGHT_ARTIFACT_FIX",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.48625676579, 0.38454173877, 0.30002108779],
    ),
    (
        "ACES-LMT - ACES 1.3 Reference Gamut Compression",
        1.0e-6,
        &[0.5, 0.4, -0.3],
        &[0.54812347889, 0.42805567384, -0.00588858686],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA_1.0",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.33629957, 0.31832799, 0.22867827],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.34128153, 0.32533440, 0.24217427],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-REC709lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.33629954, 0.31832793, 0.22867827],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-REC709lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.34128147, 0.32533434, 0.24217427],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.34128150, 0.32533440, 0.24217424],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-D65_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.32699189, 0.30769098, 0.20432013],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-D60sim-D65_1.0",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.32889283, 0.31174013, 0.21453267],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D60sim-DCI_1.0",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.34226444, 0.30731421, 0.23189434],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-CINEMA-D65sim-DCI_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.33882778, 0.30572337, 0.24966924],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-REC2020lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.48334542, 0.45336276, 0.32364485],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-1000nit-15nit-P3lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.48334542, 0.45336276, 0.32364485],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-REC2020lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.50538367, 0.47084737, 0.32972121],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-2000nit-15nit-P3lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.50538367, 0.47084737, 0.32972121],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-REC2020lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.52311981, 0.48482567, 0.33447576],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-VIDEO-4000nit-15nit-P3lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.52311981, 0.48482567, 0.33447576],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-CINEMA-108nit-7.2nit-P3lim_1.1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.22214814, 0.21179835, 0.15639816],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.26260215, 0.25207460, 0.20617345],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.26260215, 0.25207475, 0.20617352],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.16253395, 0.15513620, 0.12449738],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.20592400, 0.19440512, 0.15028587],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.41039270, 0.38813815, 0.30191854],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.46536559, 0.43852845, 0.33688101],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.51225948, 0.48264498, 0.37060043],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.55653530, 0.51967967, 0.38678783],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.41039288, 0.38813818, 0.30191860],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.46536580, 0.43852842, 0.33688098],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.51225960, 0.48264492, 0.37060046],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.55653548, 0.51967967, 0.38678783],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.25147712, 0.24029461, 0.18221153],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.25373834, 0.24245527, 0.18384993],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.25712875, 0.24569492, 0.18630651],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.25373828, 0.24245520, 0.18384989],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.26332238, 0.25161314, 0.19079420],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.15705051, 0.14920059, 0.11100878],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.20469207, 0.19229385, 0.13782671],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.39655733, 0.37322620, 0.26917258],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.44968122, 0.42165339, 0.30032712],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.49499470, 0.46407115, 0.33038712],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.53778988, 0.49960214, 0.34477147],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.40185603, 0.37821317, 0.27276924],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.45568976, 0.42728746, 0.30434006],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.50160873, 0.47027206, 0.33480173],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.54497570, 0.50627774, 0.34937829],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.40185642, 0.37821338, 0.27276939],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.45569009, 0.42728764, 0.30434042],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.50160891, 0.47027206, 0.33480188],
    ),
    (
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
        1.0e-4,
        &[0.5, 0.4, 0.3],
        &[0.54497600, 0.50627792, 0.34937853],
    ),
    (
        "APPLE_LOG_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.153334766, 0.083515430, 0.032948254],
    ),
    (
        "CURVE - APPLE_LOG_to_LINEAR",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.198913991, 0.083076466024, 0.0315782763],
    ),
    (
        "ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.401621427766, 0.236455447604, 0.064830001192],
    ),
    (
        "ARRI_LOGC4_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[1.786878082249, 0.743018593362, 0.232840037656],
    ),
    (
        "CANON_CLOG2-CGAMUT_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.408435767126, 0.197486903378, 0.034204558318],
    ),
    (
        "CURVE - CANON_CLOG2_to_LINEAR",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.492082215086, 0.183195624930, 0.064213555991],
    ),
    (
        "CANON_CLOG3-CGAMUT_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.496034919950, 0.301015360499, 0.083691829261],
    ),
    (
        "CURVE - CANON_CLOG3_to_LINEAR",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.580777404788, 0.282284436009, 0.122823721131],
    ),
    (
        "PANASONIC_VLOG-VGAMUT_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.306918773245, 0.148128050597, 0.046334439047],
    ),
    (
        "RED_REDLOGFILM-RWG_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.216116808829, 0.121529105934, 0.008171766322],
    ),
    (
        "RED_LOG3G10-RWG_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.887988237100, 0.416932247547, -0.025442210717],
    ),
    (
        "SONY_SLOG3-SGAMUT3_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.342259707137, 0.172043362337, 0.057188031769],
    ),
    (
        "SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.314942672433, 0.170408017753, 0.046854940520],
    ),
    (
        "SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.35101694, 0.17165215, 0.05479717],
    ),
    (
        "SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.32222527, 0.17032611, 0.04477848],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.937245093108, 0.586817090358, 0.573498106368, 0., 0.505174310421, 1.118456082347],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709 - MIRROR NEGS",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.937245093108, 0.586817090358, 0.573498106368, -0.940082660458, 0.505174310421, 1.118456082347],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.830338272693, 0.620393283803, 0.583385370254, 0., 0.432629991358, 1.069355537167],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.2020 - MIRROR NEGS",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.830338272693, 0.620393283803, 0.583385370254, -0.696883299726, 0.432629991358, 1.069355537167],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.931739212204, 0.559058879141, 0.545230761999, 0., 0.474767926071, 1.129896956592],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_G2.2-REC.709 - MIRROR NEGS",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.931739212204, 0.559058879141, 0.545230761999, -0.934816978533, 0.474767926071, 1.129896956592],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_sRGB",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.933793573229, 0.564092030327, 0.550040502218, -11.142147651136028, 0.477958897494, 1.124971166876],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_sRGB - MIRROR NEGS",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.933793573229, 0.564092030327, 0.550040502218, -0.936787206783, 0.477958897494, 1.124971166876],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-DCI-BFD",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.908856342287, 0.627840575107, 0.608053675805],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.896805202281, 0.627254277624, 0.608228132100, 0., 0.493163009212, 1.069368427937],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D65 - MIRROR NEGS",
        1.0e-6,
        &[0.5, 0.4, 0.3, -0.05, 0.05, 1.25],
        &[0.896805202281, 0.627254277624, 0.608228132100, -0.859521292874, 0.493163009212, 1.069368427937],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_G2.6-P3-D60-BFD",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.892433142142, 0.627011653770, 0.608093643982],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.740738422348, 0.679816639411, 0.608609083713],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_DisplayP3",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.882580907776, 0.581526360743, 0.5606367050000],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.882580907776, 0.581526360743, 0.5606367050000],
    ),
    (
        "CURVE - ST-2084_to_LINEAR",
        4.0e-5,
        &[0.5, 0.4, 0.3, -0.1, -0.3, 1.01],
        &[0.922457089941, 0.324479178538, 0.100382263105, -0.0032456566, -0.10038226, 110.045776],
    ),
    (
        "CURVE - LINEAR_to_ST-2084",
        1.0e-5,
        &[0.5, 0.4, 0.3, -0.1, 101.0, 0.2],
        &[0.440281573420, 0.419284117712, 0.392876186489, -0.299699098, 1.00104129, 0.357012421],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ",
        1.0e-5,
        &[0.5, 0.4, 0.3, -0.1, 1.01, 0.2],
        &[0.464008302136, 0.398157119110, 0.384828370950, -0.454744577, 0.562376201, 0.328883916],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65",
        1.0e-5,
        &[0.5, 0.4, 0.3, -0.1, 1.01, 0.2],
        &[0.479939091128, 0.392091860770, 0.384886051856, -0.532302439, 0.572011411, 0.307887018],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65",
        1.0e-6,
        &[0.5, 0.4, 0.3],
        &[0.440281573420, 0.419284117712, 0.392876186489],
    ),
    (
        "CURVE - HLG-OETF-INVERSE",
        1.0e-5,
        &[0.5, 0.4, 0.3, -0.7, 1.2, 0.9],
        &[0.25, 0.16, 0.09, -0.618367240391, 9.032932830300, 1.745512772886],
    ),
    (
        "CURVE - HLG-OETF",
        1.0e-5,
        &[0.5, 0.4, 0.3, -0.1, 10.0, 0.2],
        &[0.656409985167, 0.608926718364, 0.544089493962, -0.316227766017, 1.218326006877, 0.4472135955],
    ),
    (
        "DISPLAY - CIE-XYZ-D65_to_REC.2100-HLG-1000nit",
        6.0e-5,
        &[0.5, 0.4, 0.3, -0.1, 1.01, 0.2],
        &[0.5649694, 0.4038837, 0.3751478, -0.505630434, 0.738133013, 0.251128823],
    ),
];

/// Port of `OCIO_ADD_TEST(Builtins, validate)` @ v2.5.2.
#[test]
fn validate() {
    use crate::transforms::builtins::builtin_transform_registry::BuiltinTransformRegistry;

    let reg = BuiltinTransformRegistry::get();

    for index in 0..reg.num_builtins() {
        let name = std::str::from_utf8(reg.builtin_style(index).unwrap()).unwrap();
        // `UnitTestValues[name]`: empty values for a name the table misses.
        let values = UNIT_TEST_VALUES
            .iter()
            .find(|(style, ..)| *style == name)
            .map(|&(_, threshold, in_, out)| (threshold, in_, out))
            .unwrap_or((0.0, &[], &[]));

        if values.1.is_empty() || values.2.is_empty() {
            panic!("For the built-in transform '{name}' the values are missing.");
        } else if values.1.len() != values.2.len() {
            panic!("For the built-in transform '{name}' the input and output values do not match.");
        } else if (values.1.len() % 3) != 0 {
            panic!("For the built-in transform '{name}' only RGB values are supported.");
        } else {
            validate_builtin_transform(name, values.1, values.2, values.0);
        }
    }

    // The above checks if a test values is missing, but not if there are test values
    // that don't have an associated built-in.
    assert_eq!(UNIT_TEST_VALUES.len(), reg.num_builtins());
}

/// Port of `ValidateDisplayViewRoundTrip` (BuiltinTransform_tests.cpp:775-866 @ v2.5.2).
#[track_caller]
fn validate_display_view_round_trip(
    display_style: &str,
    view_style: &str,
    scale: f32,
    error_threshold: f32,
    difficult_items: &[usize],
    difficult_threshold: f32,
) {
    use crate::transforms::group_transform::GroupTransform;
    use ocio_ops::image_desc::PackedImageDesc;
    use ocio_ops::open_color_types::OptimizationFlags;
    use ocio_ops::ops::lut3d::lut3d_op::{Lut3DOrder, generate_identity_lut3d};

    // Built-in transform for the display.
    let mut display_builtin = BuiltinTransform::new();
    display_builtin.set_style(display_style).unwrap();
    display_builtin.validate().unwrap();
    let mut display_builtin_inv = display_builtin.clone();
    display_builtin_inv.set_direction(TransformDirection::Inverse);

    // Built-in transform for the view.
    let mut view_builtin = BuiltinTransform::new();
    view_builtin.set_style(view_style).unwrap();
    view_builtin.validate().unwrap();
    let mut view_builtin_inv = view_builtin.clone();
    view_builtin_inv.set_direction(TransformDirection::Inverse);

    // Assemble inverse and forward transform into a group transform that goes from
    // display code values to ACES and back to code values.
    let mut group = GroupTransform::new();
    group.append_transform(Transform::from(display_builtin_inv));
    group.append_transform(Transform::from(view_builtin_inv));
    group.append_transform(Transform::from(view_builtin));
    group.append_transform(Transform::from(display_builtin));

    // Create a Processor.
    let proc = raw_processor(Transform::from(group));

    // Create a CPUProcessor.
    // Use optimization none to avoid replacing inv/fwd pairs and avoid fast pow for the display.
    // (Though actually, the clamp to AP1 between the FixedFunctions avoids the optimization anyway.)
    let cpu = proc
        .optimized_cpu_processor(OptimizationFlags::NONE)
        .unwrap();

    // Create a 7 x 7 x 7 grid of RGBA values.
    const LUT_SIZE: usize = 7;
    const NUM_CHANNELS: usize = 4;
    let num_samples = LUT_SIZE * LUT_SIZE * LUT_SIZE;
    let mut input_32f = vec![0.0f32; num_samples * NUM_CHANNELS];
    let mut output_32f = vec![0.0f32; num_samples * NUM_CHANNELS];

    generate_identity_lut3d(
        &mut input_32f,
        LUT_SIZE as i32,
        NUM_CHANNELS as i32,
        Lut3DOrder::FastRed,
    )
    .unwrap();

    // Scale the grid of points, which is necessary when testing the ST-2084/PQ displays
    // since the transforms are only designed to process up to a maximum luminance level.
    for value in input_32f.iter_mut() {
        *value *= scale;
    }

    // Process the values.
    {
        let in_desc = PackedImageDesc::new(&input_32f[..], num_samples, 1, 4).unwrap();
        let mut out_desc = PackedImageDesc::new(&mut output_32f[..], num_samples, 1, 4).unwrap();
        cpu.apply_src_dst(&in_desc, &mut out_desc).unwrap();
    }

    // Check if values are within tolerance.
    for idx in (0..num_samples * 4).step_by(4) {
        let is_difficult = difficult_items.contains(&idx);
        let tol = if is_difficult {
            difficult_threshold
        } else {
            error_threshold
        };

        let equal_rel_r = equal_with_safe_rel_error(output_32f[idx], input_32f[idx], tol, 1.0);
        let equal_rel_g =
            equal_with_safe_rel_error(output_32f[idx + 1], input_32f[idx + 1], tol, 1.0);
        let equal_rel_b =
            equal_with_safe_rel_error(output_32f[idx + 2], input_32f[idx + 2], tol, 1.0);
        assert!(
            equal_rel_r && equal_rel_g && equal_rel_b,
            "Index: {idx} - Tol.: {tol}\n - Expected: {}, {}, {}\n - Actual:   {}, {}, {}",
            input_32f[idx],
            input_32f[idx + 1],
            input_32f[idx + 2],
            output_32f[idx],
            output_32f[idx + 1],
            output_32f[idx + 2],
        );
    }
}

/// Port of `OCIO_ADD_TEST(Builtins, aces2_displayview_roundtrip)` @ v2.5.2.
#[test]
fn aces2_displayview_roundtrip() {
    // Perform a round-trip test from display code-values to ACES and back to code values.
    // This uses a 7 x 7 x 7 grid of RGB values.

    validate_display_view_round_trip(
        "DISPLAY - CIE-XYZ-D65_to_REC.1886-REC.709",
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
        1.0,   // scale factor
        0.004, // tolerance
        &[],
        0.,
    );

    validate_display_view_round_trip(
        "DISPLAY - CIE-XYZ-D65_to_DisplayP3",
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
        1.0,   // scale factor
        0.001, // tolerance
        &[],
        0.,
    );

    validate_display_view_round_trip(
        "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65",
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
        // Need to lower the max value from 1000 to 990 nits.
        0.7507,                      // scale factor = 990 nits
        0.005,                       // main tolerance
        &[168, 196, 364, 392, 1344], // difficult values
        0.03,                        // tolerance for difficult values
    );

    validate_display_view_round_trip(
        "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65",
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
        // Need to lower the max value from 4000 to 3860 nits.
        0.8987, // scale factor = 3860 nits
        0.007,  // main tolerance
        &[
            168, 196, 392, 396, 588, 592, 952, 1148, 1196, 1200, 1260, 1288,
        ],
        0.2, // tolerance for difficult values
    );

    // TODO: The Rec.2100 transforms have too many values that don't invert to easily validate.
    // (Upstream leaves two more round trips, through "DISPLAY - CIE-XYZ-D65_to_REC.2100-PQ",
    // commented out.)
}

/// Port of `OCIO_ADD_TEST(Builtins, aces2_Aab_to_RGB_nan)` @ v2.5.2.
#[test]
fn aces2_aab_to_rgb_nan() {
    use crate::transforms::group_transform::GroupTransform;

    let display_style = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65";
    let view_style = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0";

    // Built-in transform for the display.
    let mut display_builtin_inv = BuiltinTransform::new();
    display_builtin_inv.set_style(display_style).unwrap();
    display_builtin_inv.set_direction(TransformDirection::Inverse);

    // Built-in transform for the view.
    let mut view_builtin_inv = BuiltinTransform::new();
    view_builtin_inv.set_style(view_style).unwrap();
    view_builtin_inv.set_direction(TransformDirection::Inverse);

    let mut group = GroupTransform::new();
    group.append_transform(Transform::from(display_builtin_inv));
    group.append_transform(Transform::from(view_builtin_inv));

    // Create a Processor.
    let proc = raw_processor(Transform::from(group));

    // Create a CPUProcessor.
    let cpu = proc.default_cpu_processor().unwrap();

    // This value produced a NaN prior to the Aab_to_RGB fix.
    let mut pixel = [0.89942779f32, 0.89942779, 0.89942779];

    cpu.apply_rgb(&mut pixel).unwrap();

    assert!(!pixel[0].is_nan());
    assert!(!pixel[1].is_nan());
    assert!(!pixel[2].is_nan());

    // FIXME: This gives a wildly different value on macOS ARM processors:
    // { 275.387238, 814.321838, 963.631836 }
    // (Upstream leaves its three ValidateValues of the pixel commented out.)
}
