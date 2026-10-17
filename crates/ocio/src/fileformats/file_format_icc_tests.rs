// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the ICC format: `tests/cpu/fileformats/FileFormatICC_tests.cpp` @ v2.5.2 (but
//! `types` and `endian`, with the SampleICC subset, `icc_profile_reader_tests.rs`).

use super::*;
use crate::context::Context;
use crate::fileformats::test_utils::{build_ops_test, load_test_file};
use ocio_ops::open_color_types::OptimizationFlags;
use ocio_testkit::upstream::check_close;

/// Port of `LoadICCFile` (FileFormatICC_tests.cpp:25-29 @ v2.5.2).
fn load_icc_file(file_name: &str) -> Result<LocalCachedFile> {
    load_test_file(file_name, OpenMode::Binary, |stream, path| {
        LocalFileFormat.read_file(stream, path)
    })
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, test_file)` @ v2.5.2.
#[test]
// Upstream's literals, as written.
#[allow(clippy::excessive_precision)]
fn test_file() {
    {
        // This example uses a profile with a 1024-entry LUT for the TRC.

        let icc_file_name = "icc-test-3.icm";
        let context = Context::new();

        let mut ops = OpVec::new();
        build_ops_test(
            &mut ops,
            icc_file_name,
            &context,
            TransformDirection::Forward,
        )
        .unwrap();
        ops.validate().unwrap();
        assert_eq!(4, ops.len());
        assert_eq!("<FileNoOp>", ops[0].get_info());
        assert_eq!("<MatrixOffsetOp>", ops[1].get_info());
        assert_eq!("<MatrixOffsetOp>", ops[2].get_info());
        assert_eq!("<Lut1DOp>", ops[3].get_info());

        // No-ops are removed even without any optimizations.
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(3, ops.len());

        let v0 = [1.0f32, 0.0, 0.0, 0.0];
        let v1 = [0.0f32, 1.0, 0.0, 0.0];
        let v2 = [0.0f32, 0.0, 1.0, 0.0];
        let v3 = [0.0f32, 0.0, 0.0, 1.0];

        let mut tmp = v0;
        ops[0].apply(&mut tmp).unwrap();
        assert_eq!(1.04788959f32, tmp[0]);
        assert_eq!(0.0295844227f32, tmp[1]);
        assert_eq!(-0.00925218873f32, tmp[2]);
        assert_eq!(0.0f32, tmp[3]);

        tmp = v1;
        ops[0].apply(&mut tmp).unwrap();
        assert_eq!(0.0229206420f32, tmp[0]);
        assert_eq!(0.990481913f32, tmp[1]);
        assert_eq!(0.0150730424f32, tmp[2]);
        assert_eq!(0.0f32, tmp[3]);

        tmp = v2;
        ops[0].apply(&mut tmp).unwrap();
        assert_eq!(-0.0502183065f32, tmp[0]);
        assert_eq!(-0.0170795303f32, tmp[1]);
        assert_eq!(0.751668930f32, tmp[2]);
        assert_eq!(0.0f32, tmp[3]);

        tmp = v3;
        ops[0].apply(&mut tmp).unwrap();
        assert_eq!(0.0f32, tmp[0]);
        assert_eq!(0.0f32, tmp[1]);
        assert_eq!(0.0f32, tmp[2]);
        assert_eq!(1.0f32, tmp[3]);

        tmp = v0;
        ops[1].apply(&mut tmp).unwrap();
        assert_eq!(3.13411215332385f32, tmp[0]);
        assert_eq!(-0.978787296139183f32, tmp[1]);
        assert_eq!(0.0719830443856949f32, tmp[2]);
        assert_eq!(0.0f32, tmp[3]);

        tmp = v1;
        ops[1].apply(&mut tmp).unwrap();
        assert_eq!(-1.61739245955187f32, tmp[0]);
        assert_eq!(1.91627958642662f32, tmp[1]);
        assert_eq!(-0.228985850247545f32, tmp[2]);
        assert_eq!(0.0f32, tmp[3]);

        tmp = v2;
        ops[1].apply(&mut tmp).unwrap();
        assert_eq!(-0.49063340456472f32, tmp[0]);
        assert_eq!(0.033454714231382f32, tmp[1]);
        assert_eq!(1.4053851315845f32, tmp[2]);
        assert_eq!(0.0f32, tmp[3]);

        tmp = v3;
        ops[1].apply(&mut tmp).unwrap();
        assert_eq!(0.0f32, tmp[0]);
        assert_eq!(0.0f32, tmp[1]);
        assert_eq!(0.0f32, tmp[2]);
        assert_eq!(1.0f32, tmp[3]);

        // Knowing the LUT has 1024 elements and is inverted, verify values for a given index
        // are converted to index * step
        let error = 1e-5f32;

        // value at index 200
        tmp[0] = 0.0317235067;
        tmp[1] = 0.0317235067;
        tmp[2] = 0.0317235067;
        ops[2].apply(&mut tmp).unwrap();
        check_close(200.0f32 / 1023.0f32, tmp[0], error);
        check_close(200.0f32 / 1023.0f32, tmp[1], error);
        check_close(200.0f32 / 1023.0f32, tmp[2], error);

        // Get cached file to access LUT size
        let icc_file = load_icc_file(icc_file_name).unwrap();
        let lut = icc_file.lut.as_ref().expect("a LUT");

        assert_eq!(lut.get_file_output_bit_depth(), BitDepth::Uint16);

        let lut_array = lut.get_array();
        assert_eq!(1024, lut_array.get_length());

        let values = lut_array.get_values();
        assert_eq!(0.0317235067f32, values[200 * 3]);
        assert_eq!(0.0317235067f32, values[200 * 3 + 1]);
        assert_eq!(0.0317235067f32, values[200 * 3 + 2]);
    }

    {
        // This test uses a profile where the TRC is a 1-entry curve, to be interpreted as a
        // gamma value.

        let icc_file = load_icc_file("icc-test-1.icc").unwrap();
        assert!(icc_file.lut.is_none()); // No 1D LUT.

        let expected = [
            0.609741211f32,
            0.205276489,
            0.149185181,
            0.0,
            0.311111450,
            0.625671387,
            0.0632171631,
            0.0,
            0.0194702148,
            0.0608673096,
            0.744567871,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        // `OCIO_CHECK_EQUAL(float, double)`: the float compares as a double.
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(f64::from(e), icc_file.matrix44[i], "{i}");
        }

        assert_eq!(2.19921875f32, icc_file.gamma_rgb[0]);
        assert_eq!(2.19921875f32, icc_file.gamma_rgb[1]);
        assert_eq!(2.19921875f32, icc_file.gamma_rgb[2]);
        assert_eq!(1.0f32, icc_file.gamma_rgb[3]);
    }

    {
        // This test uses a profile where the TRC is a parametric curve of type 0 (a single
        // gamma value).

        let icc_file = load_icc_file("icc-test-2.pf").unwrap();
        assert!(icc_file.lut.is_none()); // No 1D LUT.

        let expected = [
            0.504470825f32,
            0.328125000,
            0.131607056,
            0.0,
            0.264923096,
            0.682678223,
            0.0523834229,
            0.0,
            0.0144805908,
            0.0871734619,
            0.723556519,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(f64::from(e), icc_file.matrix44[i], "{i}");
        }

        assert_eq!(2.17384338f32, icc_file.gamma_rgb[0]);
        assert_eq!(2.17384338f32, icc_file.gamma_rgb[1]);
        assert_eq!(2.17384338f32, icc_file.gamma_rgb[2]);
        assert_eq!(1.0f32, icc_file.gamma_rgb[3]);
    }

    {
        // This test uses profiles where the TRC is a parametric curve of type 1-4.

        for icc_file_name in [
            "icc-test-pc1.icc",
            "icc-test-pc2.icc",
            "icc-test-pc3.icc",
            "icc-test-pc4.icc",
        ] {
            let icc_file = load_icc_file(icc_file_name).unwrap();
            let lut = icc_file.lut.as_ref().expect("a LUT");
            assert_eq!(lut.get_file_output_bit_depth(), BitDepth::F32);
            assert_eq!(1024, lut.get_array().get_length());
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, test_apply)` @ v2.5.2.
#[test]
fn test_apply() {
    let context = Context::new();
    {
        // This test uses a profile where the TRC is a 1024 element LUT.

        let icc_file_name = "icc-test-3.icm";
        let mut ops = OpVec::new();
        build_ops_test(
            &mut ops,
            icc_file_name,
            &context,
            TransformDirection::Inverse,
        )
        .unwrap();
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::LOSSLESS).unwrap();

        // apply ops
        let mut src_image = [
            -0.1f32, 0.0, 0.3, 0.0, 0.4, 0.5, 0.6, 0.5, 0.7, 1.0, 1.9, 1.0,
        ];

        let dst_image = [
            0.013221f32,
            0.005287,
            0.069636,
            0.0,
            0.188847,
            0.204323,
            0.330955,
            0.5,
            0.722887,
            0.882591,
            1.078655,
            1.0,
        ];

        let error = 1e-5f32;

        for op in ops.iter() {
            op.apply(&mut src_image).unwrap();
        }

        // compare results
        for i in 0..12 {
            check_close(src_image[i], dst_image[i], error);
        }

        // Invert the processing.

        let mut ops_inv = OpVec::new();
        build_ops_test(
            &mut ops_inv,
            icc_file_name,
            &context,
            TransformDirection::Forward,
        )
        .unwrap();
        ops_inv.finalize().unwrap();
        ops_inv.optimize(OptimizationFlags::LOSSLESS).unwrap();

        for op in ops_inv.iter() {
            // Note: This apply call hard-codes the FastLogExpPow optimization setting to false.
            op.apply(&mut src_image).unwrap();
        }

        // Currently the LUT-based TRC's clamp the values outside [0.0, 1.0], thus those values
        // won't round-trip.
        let bck_image = [
            0.0f32, 0.0, 0.3, 0.0, 0.4, 0.5, 0.6, 0.5, 0.7, 1.0, 1.0, 1.0,
        ];

        // compare results
        for i in 0..12 {
            check_close(src_image[i], bck_image[i], error);
        }
    }

    {
        // This test uses a profile where the TRC is a parametric curve of type 0 (basic
        // gamma) with gamma values {2.174, 2.174, 2.174, 1.0}.

        let icc_file_name = "icc-test-2.pf";
        let mut ops = OpVec::new();
        build_ops_test(
            &mut ops,
            icc_file_name,
            &context,
            TransformDirection::Inverse,
        )
        .unwrap();
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::LOSSLESS).unwrap();
        assert_eq!(2, ops.len());
        assert_eq!("<GammaOp>", ops[0].get_info());
        assert_eq!("<MatrixOffsetOp>", ops[1].get_info());

        // apply ops
        let src_image = [
            -0.1f32, 0.0, 0.3, 0.0, 0.4, 0.5, 0.6, 0.5, 0.7, 1.0, 1.9, 1.0,
        ];

        let dst_image = [
            0.009241f32,
            0.003003,
            0.070198,
            0.0,
            0.188392,
            0.206965,
            0.343595,
            0.5,
            1.210462,
            1.058761,
            4.003706,
            1.0,
        ];

        let mut test_image = src_image;

        for op in ops.iter() {
            op.apply(&mut test_image).unwrap();
        }

        // Compare results
        let error = 2e-5f32;

        for i in 0..12 {
            check_close(test_image[i], dst_image[i], error);
        }

        // Invert the processing.

        let mut ops_inv = OpVec::new();
        build_ops_test(
            &mut ops_inv,
            icc_file_name,
            &context,
            TransformDirection::Forward,
        )
        .unwrap();
        ops_inv.finalize().unwrap();
        ops_inv.optimize(OptimizationFlags::LOSSLESS).unwrap();
        assert_eq!(2, ops_inv.len());
        assert_eq!("<MatrixOffsetOp>", ops_inv[0].get_info());
        assert_eq!("<GammaOp>", ops_inv[1].get_info());

        for op in ops_inv.iter() {
            op.apply(&mut test_image).unwrap();
        }

        // For pure-gamma TRCs, values outside [0.0, 1.0] are NOT clamped thus those values
        // should round-trip correctly.
        let error2 = 2e-4f32;

        for i in 0..12 {
            check_close(test_image[i], src_image[i], error2);
        }
    }
}

/// Apply the ICC profile in forward then inverse direction (OCIO inverted interpretation)
/// and compare against expected values at each steps. Expects RGBA pixel layout.
///
/// Port of `ValidateRoundtripProfile` (FileFormatICC_tests.cpp:372-439 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn validate_roundtrip_profile(
    icc_file_name: &str,
    num_pixels: usize,
    src_image: &mut [f32],
    dst_image: &[f32],
    bck_image: &[f32],
    fwd_op_idx: i32,
    bck_op_idx: i32,
    error: f32,
    error_bck: f32,
) {
    let context = Context::new();
    let pixels = num_pixels * 4;

    // PCS to Device direction
    let mut ops = OpVec::new();
    build_ops_test(
        &mut ops,
        icc_file_name,
        &context,
        TransformDirection::Forward,
    )
    .unwrap();
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::LOSSLESS).unwrap();

    // Apply ops
    if fwd_op_idx >= 0 {
        ops[fwd_op_idx as usize]
            .apply(&mut src_image[..pixels])
            .unwrap();
    } else {
        for op in ops.iter() {
            op.apply(&mut src_image[..pixels]).unwrap();
        }
    }

    // Compare results
    for i in 0..pixels {
        check_close(src_image[i], dst_image[i], error);
    }

    // Invert the processing.

    // Device to PCS direction
    let mut ops_inv = OpVec::new();
    build_ops_test(
        &mut ops_inv,
        icc_file_name,
        &context,
        TransformDirection::Inverse,
    )
    .unwrap();
    ops_inv.finalize().unwrap();
    ops_inv.optimize(OptimizationFlags::LOSSLESS).unwrap();

    // apply ops
    if bck_op_idx >= 0 {
        ops_inv[bck_op_idx as usize]
            .apply(&mut src_image[..pixels])
            .unwrap();
    } else {
        for op in ops_inv.iter() {
            op.apply(&mut src_image[..pixels]).unwrap();
        }
    }

    // Compare results
    for i in 0..pixels {
        check_close(src_image[i], bck_image[i], error_bck);
    }
}

/// The source pixels of the parametric curve tests.
const PARA_SRC: [f32; 32] = [
    -1.0, -1.0, -1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.02, 0.02, 0.02, 1.0, 0.18, 0.18, 0.18, 1.0, 0.5,
    0.5, 0.5, 1.0, 0.75, 0.75, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 1.0,
];

/// Grey pixels of `values`, alpha 1.
fn greys(values: &[f32]) -> Vec<f32> {
    values.iter().flat_map(|&v| [v, v, v, 1.0]).collect()
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, test_apply_para_t1)` @ v2.5.2.
#[test]
fn test_apply_para_t1() {
    // Check processing of ParaCurve type 1.
    // g = 2.4, a = 1.1, b = -0.1
    let mut src_image = PARA_SRC;
    let dst_image = greys(&[
        0.09090909, 0.09090909, 0.26902518, 0.53586119, 0.77196938, 0.89732236, 1.0, 1.0,
    ]);
    // Negative and values above 1.0 are clamped by the LUT and won't round-trip.
    let bck_image = greys(&[0.0, 0.0, 0.02, 0.18, 0.5, 0.75, 1.0, 1.0]);
    validate_roundtrip_profile(
        "icc-test-pc1.icc",
        7,
        &mut src_image,
        &dst_image,
        &bck_image,
        1,
        0,
        2e-5,
        2e-5,
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, test_apply_para_t2)` @ v2.5.2.
#[test]
fn test_apply_para_t2() {
    // Check processing of ParaCurve type 2.
    // g = 2.4, a = 1.057, b = -0.1, c = 0.1
    let mut src_image = PARA_SRC;
    let dst_image = greys(&[
        0.09481915, 0.09481915, 0.09481915, 0.42486829, 0.74041277, 0.88520885, 1.0, 1.0,
    ]);
    // Values below the curve flat segment and above 1.0 are clamped by the LUT and won't
    // round-trip.
    let bck_image = greys(&[0.1, 0.1, 0.1, 0.18, 0.5, 0.75, 1.0, 1.0]);
    validate_roundtrip_profile(
        "icc-test-pc2.icc",
        7,
        &mut src_image,
        &dst_image,
        &bck_image,
        1,
        0,
        2e-5,
        2e-5,
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, test_apply_para_t3)` @ v2.5.2.
#[test]
fn test_apply_para_t3() {
    // Check processing of ParaCurve type 3.
    // g = 2.4, a = 1/1.055, b = 0.055/1.055, c = 1/12.92, d = 0.04045
    let mut src_image = PARA_SRC;
    let dst_image = greys(&[
        0.0, 0.0, 0.15170372, 0.46136194, 0.73536557, 0.88083965, 1.0, 1.0,
    ]);
    // Negative and values above 1.0 are clamped by the LUT and won't round-trip.
    let bck_image = greys(&[0.0, 0.0, 0.02, 0.18, 0.5, 0.75, 1.0, 1.0]);
    validate_roundtrip_profile(
        "icc-test-pc3.icc",
        7,
        &mut src_image,
        &dst_image,
        &bck_image,
        1,
        0,
        2e-5,
        2e-5,
    );
}

/// Port of `OCIO_ADD_TEST(FileFormatICC, test_apply_para_t4)` @ v2.5.2.
#[test]
fn test_apply_para_t4() {
    // Check processing of ParaCurve type 4.
    // g = 2.4, a = 0.905, b = 0.052, c = 0.073, d = 0.04, e = 0.1, f = 0.1
    let mut src_image = PARA_SRC;
    let dst_image = greys(&[0.0, 0.0, 0.0, 0.32816601, 0.69675589, 0.86589807, 1.0, 1.0]);
    // Values below the forward minimum and above 1.0 are clamped by the LUT and won't
    // round-trip.
    let bck_image = greys(&[0.1, 0.1, 0.1, 0.18, 0.5, 0.75, 1.0, 1.0]);
    validate_roundtrip_profile(
        "icc-test-pc4.icc",
        7,
        &mut src_image,
        &dst_image,
        &bck_image,
        1,
        0,
        4e-5,
        4e-5,
    );
}

/// `lround` is the platform's `lroundf`: on the values around the integers it rounds, the
/// halves, the bounds of a 32-bit and a 64-bit `long`, the infinities and NaNs, and every
/// 997th `f32` bit pattern.
#[test]
fn lround_is_the_platforms_lroundf() {
    let mut values: Vec<f32> = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.5,
        0.49999997,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -f32::NAN,
        f32::from_bits(0x7f80_0001),
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
    ];
    for bound in [2_147_483_648.0f32, 9_223_372_036_854_775_808.0] {
        for sign in [1.0f32, -1.0] {
            let b = (sign * bound).to_bits();
            for d in 0..4 {
                values.push(f32::from_bits(b - d));
                values.push(f32::from_bits(b + d));
            }
        }
    }
    values.extend((0..=u32::MAX).step_by(997).map(f32::from_bits));
    for v in values {
        assert_eq!(
            lround(v),
            ocio_testkit::crt::lroundf_c(v),
            "{v:e} ({:#010x})",
            v.to_bits()
        );
    }
}
