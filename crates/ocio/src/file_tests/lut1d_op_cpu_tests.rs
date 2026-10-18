// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/ops/lut1d/Lut1DOpCPU_tests.cpp @ v2.5.2 that read files
//! (`CreateFileTransform`, `BuildOpsTest`). The others are
//! `crates/ocio-ops/src/ops/lut1d/lut1d_op_cpu_tests.rs`.

use std::sync::Arc;

use ocio_ops::cpu_info::{
    CpuInfo, X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512, X86_CPU_FLAG_SSE2,
};
use ocio_ops::half::f16;
use ocio_ops::image_desc::{AUTO_STRIDE, PackedImageDesc};
use ocio_ops::math_utils::{floats_differ, std_powf};
use ocio_ops::op::OpVec;
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut1d::lut1d_op_cpu::get_lut1d_renderer_for_cpu;
use ocio_ops::ops::lut1d::lut1d_op_data::{Lut1DOpData, make_fast_lut1d_from_inverse};
use ocio_testkit::upstream::{check_close, equal_with_safe_rel_error};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::test_utils::{build_ops_test, get_test_files_dir};
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::file_transform::FileTransform;

/// `OPTIMIZATION_DEFAULT` without `OPTIMIZATION_LUT_INV_FAST`.
///
/// Port of `DefaultNoLutInvFast` (Lut1DOpCPU_tests.cpp:1283-1284 @ v2.5.2).
const DEFAULT_NO_LUT_INV_FAST: OptimizationFlags =
    OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::LUT_INV_FAST.0);

/// `expected: <expected> != actual: <actual>`, both as a stream prints a `float`.
///
/// Port of `GetErrorMessage` (Lut1DOpCPU_tests.cpp:1454-1459 @ v2.5.2).
fn get_error_message(expected: f32, actual: f32) -> String {
    let mut oss = ocio_ops::cfmt::OStringStream::new(ocio_ops::cfmt::Crt::NATIVE);
    oss.put_str("expected: ");
    oss.put_f32(expected);
    oss.put_str(" != actual: ");
    oss.put_f32(actual);
    oss.to_string_lossy().into_owned()
}

/// Validates and finalizes the inverse LUT, and returns its fast forward LUT.
///
/// Port of `FastFromInverse` (Lut1DOpCPU_tests.cpp:213-222 @ v2.5.2).
fn fast_from_inverse(inv_lut_data: &mut Lut1DOpData) -> Lut1DOpData {
    inv_lut_data.validate().unwrap();
    inv_lut_data.finalize().unwrap();
    make_fast_lut1d_from_inverse(inv_lut_data).unwrap()
}

/// A file transform of the test file `file_name`.
///
/// Port of `CreateFileTransform` (tests/cpu/UnitTestUtils.cpp:39-48 @ v2.5.2).
fn create_file_transform(file_name: &str) -> FileTransform {
    let mut file_transform = FileTransform::new();
    file_transform.set_src(format!("{}/{file_name}", get_test_files_dir()));
    file_transform
}

/// The processor of `file_transform` in a new config (`Config::Create`).
fn processor(config: &Config, file_transform: &FileTransform) -> Arc<Processor> {
    config
        .processor(&Transform::File(file_transform.clone()))
        .unwrap()
}

/// Applies `cpu` to the RGBA F32 pixels of `src` into `dst`.
fn apply_f32(cpu: &ocio_ops::cpu_processor::CpuProcessor, src: &[f32], dst: &mut [f32]) {
    let n = src.len() / 4;
    let src_desc = PackedImageDesc::new(src, n, 1, 4).unwrap();
    let mut dst_desc = PackedImageDesc::new(dst, n, 1, 4).unwrap();
    cpu.apply_src_dst(&src_desc, &mut dst_desc).unwrap();
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_special_values)` @ v2.5.2.
#[test]
fn lut_1d_special_values() {
    // Create empty Config to use.
    let config = Config::new().unwrap();

    let ctf_lut = "lut1d_infinity.ctf";
    let file_transform = create_file_transform(ctf_lut);

    // Get the processor corresponding to the transform.
    let proc = processor(&config, &file_transform);

    // This test should use the "interpolation" renderer path.
    let cpu_fwd = proc.default_cpu_processor().unwrap();

    let step = 1.0f32 / 65535.0f32;

    let in_image = [
        0.0f32,
        0.5f32 * step,
        step,
        1.0f32,
        3000.0f32 * step,
        32000.0f32 * step,
        65535.0f32 * step,
        1.0f32,
    ];

    let mut out_image = vec![-12.345f32; 2 * 4];
    apply_f32(&cpu_fwd, &in_image, &mut out_image);

    // -Inf is mapped to -MAX_FLOAT
    let negmax = -f32::MAX;

    let lut_elem_1 = -3216.000488281f32;
    let lut_elem_3000 = -539.565734863f32;

    let rtol = std_powf(2.0f32, -14.0f32);

    // LUT output bit-depth is 12i so normalize to F32.
    let out_range = 4095.0f32;

    check_close(out_image[0], negmax, rtol);
    check_close(
        out_image[1],
        (lut_elem_1 / out_range + negmax) * 0.5f32,
        rtol,
    );
    check_close(out_image[2], lut_elem_1 / out_range, rtol);
    assert_eq!(out_image[3], 1.0f32);

    check_close(out_image[4], lut_elem_3000 / out_range, rtol);
    check_close(out_image[5], negmax, rtol);
    check_close(out_image[6], negmax, rtol);
    assert_eq!(out_image[7], 1.0f32);
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_hd_above_half_max)` @ v2.5.2.
#[test]
fn lut_1d_hd_above_half_max() {
    // Test the processing of half-domain Lut1D for float input values that are greater than
    // HALF_MAX but round down to HALF_MAX. These are the values 65504 < x < 65520. In other
    // words, half(65519) rounds down to 65504 and half(65520) rounds up to Inf. There was a
    // bug where these values were not processed correctly.

    let config = Config::new().unwrap();

    let file_transform = create_file_transform("lut1d_hd_16f_16i_1chan.ctf");

    let proc = processor(&config, &file_transform);

    let cpu_fwd = proc.default_cpu_processor().unwrap();

    let in_image = [
        65505.0f32, 65519.0, 65520.0, 0.0, //
        -65505.0, -65519.0, -65520.0, 1.0,
    ];

    let mut out_image = vec![-12.345f32; 2 * 4];
    apply_f32(&cpu_fwd, &in_image, &mut out_image);

    let rtol = 1e-5f32;
    check_close(out_image[0], 0.7785763f32, rtol);
    check_close(out_image[1], 0.7785763f32, rtol);
    check_close(out_image[2], 0.7785763f32, rtol);
    assert_eq!(out_image[3], 0.0f32);

    check_close(out_image[4], 0.0f32, rtol);
    check_close(out_image[5], 0.0f32, rtol);
    check_close(out_image[6], 0.0f32, rtol);
    assert_eq!(out_image[7], 1.0f32);
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_hue_adjust)` @ v2.5.2.
#[test]
fn lut_1d_hue_adjust() {
    // Create empty Config to use.
    let config = Config::new().unwrap();

    let file_transform = create_file_transform("lut1d_1024_hue_adjust_test.ctf");

    // Get the processor corresponding to the transform.
    let proc = processor(&config, &file_transform);

    // This test should use the "lookup" renderer path.
    let cpu_fwd = proc
        .optimized_cpu_processor_with_bit_depths(
            BitDepth::Uint16,
            BitDepth::Uint16,
            DEFAULT_NO_LUT_INV_FAST,
        )
        .unwrap();

    const NB_PIXELS: usize = 2;
    // TODO: use UINT10 when implemented by ImageDesc.
    let in_image: [u16; NB_PIXELS * 4] = [
        (100 * 65535 / 1023) as u16,
        (500 * 65535 / 1023) as u16,
        (800 * 65535 / 1023) as u16,
        (200 * 65535 / 1023) as u16,
        (400 * 65535 / 1023) as u16,
        (700 * 65535 / 1023) as u16,
        (300 * 65535 / 1023) as u16,
        (1023 * 65535 / 1023) as u16,
    ];

    let u16_size = std::mem::size_of::<u16>() as isize;
    let mut out_image = [2000u16; NB_PIXELS * 4];
    {
        let src_img_desc = PackedImageDesc::with_strides(
            &in_image[..],
            NB_PIXELS,
            1,
            4,
            BitDepth::Uint16,
            u16_size,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        let mut dst_img_desc = PackedImageDesc::with_strides(
            &mut out_image[..],
            NB_PIXELS,
            1,
            4,
            BitDepth::Uint16,
            u16_size,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        cpu_fwd
            .apply_src_dst(&src_img_desc, &mut dst_img_desc)
            .unwrap();
    }

    assert_eq!(out_image[0], 1523);
    assert_eq!(out_image[1], 33883); // Would be 31583 w/out hue adjust.
    assert_eq!(out_image[2], 58154);
    assert_eq!(out_image[3], 12812);

    assert_eq!(out_image[4], 22319); // Would be 21710 w/out hue adjust.
    assert_eq!(out_image[5], 50648);
    assert_eq!(out_image[6], 12877);
    assert_eq!(out_image[7], 65535);

    // This test should use the "interpolation" renderer path.
    let mut in_float_image = [0.0f32; 8];
    for i in 0..8 {
        in_float_image[i] = f32::from(in_image[i]) / 65535.0f32;
    }
    out_image.fill(200);

    let cpu_fwd_fast = proc
        .optimized_cpu_processor_with_bit_depths(
            BitDepth::F32,
            BitDepth::Uint16,
            OptimizationFlags::DEFAULT,
        )
        .unwrap();

    {
        let src_img_f_desc = PackedImageDesc::with_strides(
            &in_float_image[..],
            NB_PIXELS,
            1,
            4,
            BitDepth::F32,
            std::mem::size_of::<f32>() as isize,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        let mut dst_img_desc = PackedImageDesc::with_strides(
            &mut out_image[..],
            NB_PIXELS,
            1,
            4,
            BitDepth::Uint16,
            u16_size,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        cpu_fwd_fast
            .apply_src_dst(&src_img_f_desc, &mut dst_img_desc)
            .unwrap();
    }
    assert_eq!(out_image[0], 1523);
    assert_eq!(out_image[1], 33883); // Would be 31583 w/out hue adjust.
    assert_eq!(out_image[2], 58154);
    assert_eq!(out_image[3], 12812);

    assert_eq!(out_image[4], 22319); // Would be 21710 w/out hue adjust.
    assert_eq!(out_image[5], 50648);
    assert_eq!(out_image[6], 12877);
    assert_eq!(out_image[7], 65535);
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_half_domain_hue_adjust)` @ v2.5.2.
#[test]
fn lut_1d_half_domain_hue_adjust() {
    // Create empty Config to use.
    let config = Config::new().unwrap();

    let file_transform = create_file_transform("lut1d_hd_hue_adjust.ctf");

    // Get the processor corresponding to the transform.
    let proc = processor(&config, &file_transform);

    // This test should use the "interpolation" renderer path.
    let cpu_fwd = proc.default_cpu_processor().unwrap();

    let in_image = [
        0.05f32, 0.18, 1.1, 0.5, //
        2.3, 0.01, 0.3, 1.0,
    ];

    let mut out_image = vec![-1.0f32; 2 * 4];
    apply_f32(&cpu_fwd, &in_image, &mut out_image);

    let rtol = 1e-6f32;
    let min_expected = 1e-3f32;

    assert!(equal_with_safe_rel_error(
        out_image[0],
        0.54780269f32,
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[1],
        9.57448578f32, // Would be 5.0 w/out hue adjust.
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[2],
        73.45562744f32,
        rtol,
        min_expected
    ));
    assert_eq!(out_image[3], 0.5f32);

    assert!(equal_with_safe_rel_error(
        out_image[4],
        188.087067f32,
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[5],
        0.0324990489f32,
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[6],
        23.8472710f32, // Would be 11.3372078 w/out hue adjust.
        rtol,
        min_expected
    ));
    assert_eq!(out_image[7], 1.0f32);

    // This test should use the "lookup" renderer path.
    let cpu_fwd = proc
        .optimized_cpu_processor_with_bit_depths(
            BitDepth::Uint16,
            BitDepth::F32,
            DEFAULT_NO_LUT_INV_FAST,
        )
        .unwrap();

    // TODO: Use 10i when ImageDesc handles 10i.
    let in_image_int: [u16; 8] = [
        (200 * 65535 / 1023) as u16,
        (800 * 65535 / 1023) as u16,
        (500 * 65535 / 1023) as u16,
        0,
        (400 * 65535 / 1023) as u16,
        (100 * 65535 / 1023) as u16,
        (700 * 65535 / 1023) as u16,
        (1023 * 65535 / 1023) as u16,
    ];

    out_image.fill(-1.0);
    {
        let src_img_int_desc = PackedImageDesc::with_strides(
            &in_image_int[..],
            2,
            1,
            4,
            BitDepth::Uint16,
            std::mem::size_of::<u16>() as isize,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        let mut dst_img_desc = PackedImageDesc::new(&mut out_image[..], 2, 1, 4).unwrap();
        cpu_fwd
            .apply_src_dst(&src_img_int_desc, &mut dst_img_desc)
            .unwrap();
    }

    assert!(equal_with_safe_rel_error(
        out_image[0],
        5.72640753f32,
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[1],
        46.2259789f32,
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[2],
        25.9756680f32, // Would be 23.6895752 w/out hue adjust.
        rtol,
        min_expected
    ));
    assert_eq!(out_image[3], 0.0f32);

    assert!(equal_with_safe_rel_error(
        out_image[4],
        20.0804043f32, // Would be 17.0063152 w/out hue adjust.
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[5],
        1.78572845f32,
        rtol,
        min_expected
    ));
    assert!(equal_with_safe_rel_error(
        out_image[6],
        38.3760300f32,
        rtol,
        min_expected
    ));
    assert_eq!(out_image[7], 1.0f32);
}

/// The CPU flags of upstream's SIMD modes: its test runner reruns every test in each SIMD
/// mode the CPU supports (tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2), as
/// `crates/ocio-ops/src/ops/lut1d/lut1d_op_cpu_tests.rs` does for the renderers it builds.
fn simd_modes() -> Vec<CpuInfo> {
    let cpu = CpuInfo::instance();
    let mut modes = vec![cpu.with_flags(0)];
    for flag in [
        X86_CPU_FLAG_SSE2,
        X86_CPU_FLAG_AVX,
        X86_CPU_FLAG_AVX2,
        X86_CPU_FLAG_AVX512,
    ] {
        // Upstream skips a mode the CPU does not support; forcing the flag is how it selects
        // one.
        if cpu.flags & flag != 0 {
            modes.push(cpu.with_flags(flag));
        }
    }
    modes
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_hue_adjust)` @ v2.5.2.
/// In each SIMD mode ([`simd_modes`]). Upstream's renderers write out of place
/// (`apply(in, out, n)`); here they render a copy in place.
#[test]
fn lut_1d_inv_hue_adjust() {
    for cpu in simd_modes() {
        lut_1d_inv_hue_adjust_in(&cpu);
    }
}

/// [`lut_1d_inv_hue_adjust`] with the renderers `cpu` picks.
fn lut_1d_inv_hue_adjust_in(cpu: &CpuInfo) {
    let ctf_lut = "lut1d_1024_hue_adjust_test.ctf";

    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, ctf_lut, &context, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 2);
    let op_data = ops[1].data();
    assert_eq!(op_data.get_type(), OpDataType::Lut1D);
    let OpData::Lut1D(lut) = &**op_data else {
        panic!("not a Lut1D");
    };

    let mut lut_data = lut.clone();

    // Currently need to set this to 16i in order for style == FAST to pass. See comment in
    // MakeFastLut1DFromInverse.
    lut_data.set_file_output_bit_depth(BitDepth::Uint16);
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer_for_cpu(&lut_data, BitDepth::F32, BitDepth::F32, cpu).unwrap();

    let in_image = [
        0.1f32, 0.25, 0.7, 0.0, //
        0.66, 0.25, 0.81, 0.5, //
        // 0.18f, 0.80f, 0.45f, 1.f }; // This one is easier to get correct.
        0.18, 0.99, 0.45, 1.0, // Setting G way up on the s-curve is harder.
    ];
    const NB_PIXELS: usize = 3;

    let mut out_image = in_image.to_vec();
    cpu_op.apply(&mut out_image);

    // Inverse using FAST.
    let mut lut_data_inv = lut_data.inverse();
    let const_lut_inv = fast_from_inverse(&mut lut_data_inv);
    let cpu_op_inv =
        get_lut1d_renderer_for_cpu(&const_lut_inv, BitDepth::F32, BitDepth::F32, cpu).unwrap();

    let mut back_image = out_image.clone();
    cpu_op_inv.apply(&mut back_image);

    for i in 0..NB_PIXELS * 4 {
        assert!(
            !floats_differ(in_image[i], back_image[i], 130, false),
            "{}",
            get_error_message(in_image[i], back_image[i])
        );
    }

    // Repeat with EXACT.
    let mut lut_data_inv2 = lut_data.inverse();

    lut_data_inv2.validate().unwrap();
    lut_data_inv2.finalize().unwrap();
    let cpu_op_inv2 =
        get_lut1d_renderer_for_cpu(&lut_data_inv2, BitDepth::F32, BitDepth::F32, cpu).unwrap();

    let mut back_image = out_image.clone();
    cpu_op_inv2.apply(&mut back_image);

    for i in 0..NB_PIXELS * 4 {
        assert!(
            !floats_differ(in_image[i], back_image[i], 50, false),
            "{}",
            get_error_message(in_image[i], back_image[i])
        );
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_half_ctf)` @ v2.5.2.
#[test]
fn lut_1d_inv_half_ctf() {
    // Create empty Config to use.
    let config = Config::new().unwrap();

    // This ctf has increasing R & B channels and decreasing G channel. It also has flat spots.
    let mut file_transform = create_file_transform("lut1d_halfdom.ctf");

    // Get the processor corresponding to the transform.
    let proc = processor(&config, &file_transform);

    // This test should use the "interpolation" renderer path.
    let cpu_fwd = proc.default_cpu_processor().unwrap();

    let in_image: [f32; 12] = [
        1.0, 1.0, 0.5, 0.0, //
        0.001, 0.1, 4.0, 0.5, // test positive half domain of R, G, B channels
        -0.08, -1.0, -10.0, 1.0, // test negative half domain of R, G, B channels
    ];

    // Apply forward LUT.
    let mut out_image = vec![-1.0f32; 12];
    apply_f32(&cpu_fwd, &in_image, &mut out_image);

    // Apply inverse LUT.
    // Inverse using FAST.
    file_transform.set_direction(TransformDirection::Inverse);

    let proc = processor(&config, &file_transform);
    let cpu_op_inv = proc.default_cpu_processor().unwrap();

    let mut back_image = vec![-1.0f32; 12];
    apply_f32(&cpu_op_inv, &out_image, &mut back_image);

    for i in 0..12 {
        assert!(
            !floats_differ(in_image[i], back_image[i], 50, false),
            "{}",
            get_error_message(in_image[i], back_image[i])
        );
    }

    // Repeat with EXACT.
    back_image.fill(-1.0);
    let cpu_inv_fast = proc
        .optimized_cpu_processor(DEFAULT_NO_LUT_INV_FAST)
        .unwrap();
    apply_f32(&cpu_inv_fast, &out_image, &mut back_image);

    for i in 0..12 {
        assert!(
            !floats_differ(in_image[i], back_image[i], 50, false),
            "{}",
            get_error_message(in_image[i], back_image[i])
        );
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_half_fclut)` @ v2.5.2.
#[test]
fn lut_1d_inv_half_fclut() {
    // Create empty Config to use.
    let config = Config::new().unwrap();

    // TODO: Review the test to add LUT & inverse LUT together when optimization is reworked.

    // Lustre log_default.fclut. All positive halfs map to unique 16-bit ints so it's a good
    // test to see that the inverse can restore the halfs losslessly.
    let mut file_transform = create_file_transform("lut1d_hd_16f_16i_1chan.ctf");

    // Get the processor corresponding to the transform.
    let proc = processor(&config, &file_transform);
    let cpu_op = proc
        .optimized_cpu_processor_with_bit_depths(
            BitDepth::F16,
            BitDepth::F32,
            DEFAULT_NO_LUT_INV_FAST,
        )
        .unwrap();

    // Test all positive halfs (less than inf) round-trip losslessly.
    let nb_pixels = 31744usize;
    let mut in_image = vec![f16::ZERO; nb_pixels * 4];
    let mut out_image = vec![-1.0f32; nb_pixels * 4];
    for i in 0..nb_pixels {
        let h = f16::from_bits(i as u16);
        in_image[i * 4] = h;
        in_image[i * 4 + 1] = h;
        in_image[i * 4 + 2] = h;
        in_image[i * 4 + 3] = h;
    }

    let half_size = std::mem::size_of::<f16>() as isize;
    {
        let src_img_desc = PackedImageDesc::with_strides(
            &in_image[..],
            nb_pixels,
            1,
            4,
            BitDepth::F16,
            half_size,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        let mut dst_img_desc = PackedImageDesc::new(&mut out_image[..], nb_pixels, 1, 4).unwrap();
        cpu_op
            .apply_src_dst(&src_img_desc, &mut dst_img_desc)
            .unwrap();
    }

    file_transform.set_direction(TransformDirection::Inverse);

    let proc = processor(&config, &file_transform);
    let mut cpu_op_inv = proc
        .optimized_cpu_processor_with_bit_depths(
            BitDepth::F32,
            BitDepth::F16,
            DEFAULT_NO_LUT_INV_FAST,
        )
        .unwrap();

    // `half(-1.f)`.
    let minus_one = f16::from_bits(ocio_ops::imath_half::float_to_half(-1.0));
    let mut back_image = vec![minus_one; nb_pixels * 4];
    for pass in 0..2 {
        if pass == 1 {
            // Repeat with FAST.
            cpu_op_inv = proc
                .optimized_cpu_processor_with_bit_depths(
                    BitDepth::F32,
                    BitDepth::F16,
                    DEFAULT_NO_LUT_INV_FAST,
                )
                .unwrap();

            back_image.fill(minus_one);
        }
        {
            let src_desc = PackedImageDesc::new(&out_image[..], nb_pixels, 1, 4).unwrap();
            let mut back_img_desc = PackedImageDesc::with_strides(
                &mut back_image[..],
                nb_pixels,
                1,
                4,
                BitDepth::F16,
                half_size,
                AUTO_STRIDE,
                AUTO_STRIDE,
            )
            .unwrap();
            cpu_op_inv
                .apply_src_dst(&src_desc, &mut back_img_desc)
                .unwrap();
        }

        for i in 0..nb_pixels {
            for c in 0..4 {
                assert_eq!(
                    in_image[i * 4 + c].to_bits(),
                    back_image[i * 4 + c].to_bits()
                );
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_half_domain_hue_adjust)` @ v2.5.2.
#[test]
fn lut_1d_inv_half_domain_hue_adjust() {
    // Create empty Config to use.
    let config = Config::new().unwrap();

    let mut file_transform = create_file_transform("lut1d_hd_hue_adjust.ctf");

    // Get the processor corresponding to the transform.
    let proc = processor(&config, &file_transform);
    let cpu_fwd = proc.default_cpu_processor().unwrap();

    const NB_PIXELS: usize = 3;
    let in_image: [f32; NB_PIXELS * 4] = [
        0.1, 0.25, 0.7, 0.0, //
        0.66, 0.25, 0.81, 0.5, //
        0.18, 0.99, 0.45, 1.0,
    ];

    let mut out_image = vec![1.0f32; NB_PIXELS * 4];
    apply_f32(&cpu_fwd, &in_image, &mut out_image);

    // Inverse using FAST (which is part of the default optimization level).
    file_transform.set_direction(TransformDirection::Inverse);

    let proc = processor(&config, &file_transform);
    let cpu_inv = proc.default_cpu_processor().unwrap();

    let mut back_image = vec![-1.0f32; NB_PIXELS * 4];
    apply_f32(&cpu_inv, &out_image, &mut back_image);

    for i in 0..NB_PIXELS * 4 {
        assert!(
            !floats_differ(in_image[i], back_image[i], 50, false),
            "{}",
            get_error_message(in_image[i], back_image[i])
        );
    }

    // Repeat with EXACT.
    let cpu_inv_fast = proc
        .optimized_cpu_processor(DEFAULT_NO_LUT_INV_FAST)
        .unwrap();
    back_image.fill(-1.0);
    apply_f32(&cpu_inv_fast, &out_image, &mut back_image);

    for i in 0..NB_PIXELS * 4 {
        assert!(
            !floats_differ(in_image[i], back_image[i], 50, false),
            "{}",
            get_error_message(in_image[i], back_image[i])
        );
    }
}
