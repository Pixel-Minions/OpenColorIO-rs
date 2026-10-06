// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOpCPU_tests.cpp` @ v2.5.2, the tests of the forward
//! renderers that need no resampling (`Compose`, WP 2.1g); tests of the renderers' dispatch,
//! and of the 10- and 12-bit codes past the tables (docs/improvements.md, U-1). The pixels are
//! compared with the wheel's in `tests/lut1d_op_cpu_oracle.rs` and
//! `tests/lut1d_renderer_oracle.rs`.
//!
//! Upstream's test runner reruns every test in each SIMD mode the CPU supports
//! (tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2). The tests of a standard domain with float
//! input, whose renderer runs a SIMD kernel on rows of more than one pixel, run once per mode
//! here ([`simd_modes`]).

use super::*;
use crate::cpu_info::{
    X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512, X86_CPU_FLAG_SSE2,
};
use crate::ops::lut1d::lut1d_op_data::HalfFlags;
use ocio_testkit::upstream::check_close;

fn message(r: Result<Arc<dyn CpuOp>>) -> String {
    match r {
        Ok(op) => panic!("a renderer: {op:?}"),
        Err(e) => e.message().to_string(),
    }
}

/// `half(value)`.
fn half(value: f32) -> half::f16 {
    half::f16::from_bits(float_to_half(value))
}

/// `(float)value` of a half.
fn float(value: half::f16) -> f32 {
    half_to_float(value.to_bits())
}

/// The CPU flags upstream's test runner forces for each SIMD mode (`--no_accel`, `-sse2`,
/// `-avx`, `-avx2`, `-avx512`; tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2), as in
/// `lut3d_op_cpu_tests.rs`. The runner reruns the whole test binary per mode; here each test
/// body runs once per mode.
fn simd_modes() -> Vec<CpuInfo> {
    let cpu = CpuInfo::instance();
    let mut modes = vec![cpu.with_flags(0)];
    for flag in [
        X86_CPU_FLAG_SSE2,
        X86_CPU_FLAG_AVX,
        X86_CPU_FLAG_AVX2,
        X86_CPU_FLAG_AVX512,
    ] {
        // Upstream skips a mode the CPU does not support; forcing the flag is how it selects one.
        if cpu.flags & flag != 0 {
            modes.push(cpu.with_flags(flag));
        }
    }
    modes
}

/// `GetLut1DRenderer` picks a lookup for integer and half input that may use the LUT as it is,
/// a float renderer for a half domain with float input, with or without hue adjust, and the
/// hue-adjust renderer of a standard domain with float input, and the standard domain's
/// renderer, for every output bit depth; the inverse and the lookups that must resample the LUT
/// are still to come.
#[test]
fn dispatch() {
    let lut8 = Lut1DOpData::new(256).unwrap();
    let half_lut = Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, true).unwrap();
    for out in [
        BitDepth::Uint8,
        BitDepth::Uint10,
        BitDepth::Uint12,
        BitDepth::Uint16,
        BitDepth::F16,
        BitDepth::F32,
    ] {
        for (lut, depth) in [
            (&lut8, BitDepth::Uint8),
            (&half_lut, BitDepth::F16),
            (&half_lut, BitDepth::F32),
        ] {
            get_lut1d_renderer(lut, depth, out).unwrap();
        }
        for depth in [BitDepth::Uint10, BitDepth::Uint12, BitDepth::Uint16] {
            let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
            get_lut1d_renderer(&lut, depth, out).unwrap();
        }
        get_lut1d_renderer(&lut8, BitDepth::F32, out).unwrap();
        get_lut1d_scalar_renderer(&lut8, out).unwrap();
    }

    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint14, BitDepth::F32)),
        "Unsupported input bit depth"
    );
    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint8, BitDepth::Uint32)),
        "Unsupported output bit depth"
    );
    // The input bit depth is checked first.
    assert_eq!(
        message(get_lut1d_renderer(
            &lut8,
            BitDepth::Unknown,
            BitDepth::Unknown
        )),
        "Unsupported input bit depth"
    );
    let inverse = lut8.inverse();
    assert_eq!(
        message(get_lut1d_renderer(&inverse, BitDepth::Uint8, BitDepth::F32)),
        NOT_PORTED_INVERSE_RENDERER
    );
    let mut hue = lut8.clone();
    hue.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    let mut half_hue = half_lut.clone();
    half_hue.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();
    for out in [BitDepth::Uint8, BitDepth::F16, BitDepth::F32] {
        for (lut, depth) in [
            (&hue, BitDepth::Uint8),
            (&hue, BitDepth::F32),
            (&half_hue, BitDepth::F16),
            (&half_hue, BitDepth::F32),
        ] {
            get_lut1d_renderer(lut, depth, out).unwrap();
        }
    }
    assert_eq!(
        message(get_lut1d_renderer(&hue, BitDepth::Uint10, BitDepth::F32)),
        NOT_PORTED_COMPOSE
    );
    // A LUT the lookup must resample first.
    assert_eq!(
        message(get_lut1d_renderer(&lut8, BitDepth::Uint10, BitDepth::F32)),
        NOT_PORTED_COMPOSE
    );
    assert_eq!(
        message(get_lut1d_renderer(
            &half_lut,
            BitDepth::Uint16,
            BitDepth::F32
        )),
        NOT_PORTED_COMPOSE
    );
    // The scalar profile is the standard domain's.
    for lut in [&half_lut, &inverse, &hue] {
        assert!(message(get_lut1d_scalar_renderer(lut, BitDepth::F32)).contains("scalar profile"));
    }
    assert_eq!(
        message(get_lut1d_scalar_renderer(&lut8, BitDepth::Uint14)),
        "Unsupported output bit depth"
    );
}

/// `GamutMapUtils::Order3(RGB, min, mid, max)`.
fn order3_of(rgb: [f32; 3]) -> (usize, usize, usize) {
    order3(&rgb)
}

/// Port of `OCIO_ADD_TEST(GamutMapUtil, order3_test)` @ v2.5.2.
#[test]
fn order3_test() {
    let posinf = f32::INFINITY;
    let qnan = f32::NAN;

    // { A, NaN, B } with A > B test (used to be a crash).
    {
        let (min, mid, max) = order3_of([65504.0, -qnan, 0.0]);
        assert_eq!(max, 2);
        assert_eq!(mid, 1);
        assert_eq!(min, 0);
    }
    // Triple NaN test.
    {
        let (min, mid, max) = order3_of([qnan, qnan, -qnan]);
        assert_eq!(max, 2);
        assert_eq!(mid, 1);
        assert_eq!(min, 0);
    }
    // -Inf test.
    {
        let (min, mid, max) = order3_of([65504.0, -posinf, 0.0]);
        assert_eq!(max, 0);
        assert_eq!(mid, 2);
        assert_eq!(min, 1);
    }
    // Inf test.
    {
        let (min, mid, max) = order3_of([0.0, posinf, -65504.0]);
        assert_eq!(max, 1);
        assert_eq!(mid, 0);
        assert_eq!(min, 2);
    }
    // Double Inf test.
    {
        let (min, mid, max) = order3_of([posinf, posinf, -65504.0]);
        assert_eq!(max, 1);
        assert_eq!(mid, 0);
        assert_eq!(min, 2);
    }

    // Equal values.
    {
        let (min, mid, max) = order3_of([0.0, 0.0, 0.0]);
        // In this case we only really care that they are distinct and in [0,2]
        // so this test could be changed (it is ok, but overly restrictive).
        assert_eq!(max, 2);
        assert_eq!(mid, 1);
        assert_eq!(min, 0);
    }

    // Now test the six typical possibilities.
    {
        let (min, mid, max) = order3_of([3.0, 2.0, 1.0]);
        assert_eq!(max, 0);
        assert_eq!(mid, 1);
        assert_eq!(min, 2);
    }
    {
        let (min, mid, max) = order3_of([-3.0, -2.0, 1.0]);
        assert_eq!(max, 2);
        assert_eq!(mid, 1);
        assert_eq!(min, 0);
    }
    {
        let (min, mid, max) = order3_of([-3.0, 2.0, 1.0]);
        assert_eq!(max, 1);
        assert_eq!(mid, 2);
        assert_eq!(min, 0);
    }
    {
        let (min, mid, max) = order3_of([-0.3, 2.0, -1.0]);
        assert_eq!(max, 1);
        assert_eq!(mid, 0);
        assert_eq!(min, 2);
    }
    {
        let (min, mid, max) = order3_of([3.0, -2.0, 1.0]);
        assert_eq!(max, 0);
        assert_eq!(mid, 2);
        assert_eq!(min, 1);
    }
    {
        let (min, mid, max) = order3_of([3.0, -2.0, 10.0]);
        assert_eq!(max, 2);
        assert_eq!(mid, 0);
        assert_eq!(min, 1);
    }
}

/// Once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, nan_test)` @ v2.5.2.
#[test]
fn nan_test() {
    let mut lut = Lut1DOpData::new(8).unwrap();

    let values = lut.get_array_mut().get_values_mut();

    values[0] = 0.0f32;
    values[1] = 0.0f32;
    values[2] = 0.002333f32;
    values[3] = 0.0f32;
    values[4] = 0.291341f32;
    values[5] = 0.015624f32;
    values[6] = 0.106521f32;
    values[7] = 0.334331f32;
    values[8] = 0.462431f32;
    values[9] = 0.515851f32;
    values[10] = 0.474151f32;
    values[11] = 0.624611f32;
    values[12] = 0.658791f32;
    values[13] = 0.527381f32;
    values[14] = 0.685071f32;
    values[15] = 0.908501f32;
    values[16] = 0.707951f32;
    values[17] = 0.886331f32;
    values[18] = 0.926671f32;
    values[19] = 0.846431f32;
    values[20] = 1.0f32;
    values[21] = 1.0f32;
    values[22] = 1.0f32;
    values[23] = 1.0f32;
    let values = values.clone();

    for cpu in simd_modes() {
        let renderer =
            get_lut1d_renderer_for_cpu(&lut, BitDepth::F32, BitDepth::F32, &cpu).unwrap();

        let qnan = f32::NAN;
        let inf = f32::INFINITY;

        #[rustfmt::skip]
    let mut pixels: [f32; 24] = [
        qnan, 0.5, 0.3, -0.2,
        0.5, qnan, 0.3, 0.2,
        0.5, 0.3, qnan, 1.2,
        0.5, 0.3, 0.2, qnan,
        inf, inf, inf, inf,
        -inf, -inf, -inf, -inf,
    ];

        renderer.apply(&mut pixels);

        check_close(pixels[0], values[0], 1e-7f32);
        check_close(pixels[5], values[1], 1e-7f32);
        check_close(pixels[10], values[2], 1e-7f32);
        assert!(pixels[15].is_nan());
        check_close(pixels[16], values[21], 1e-7f32);
        check_close(pixels[17], values[22], 1e-7f32);
        check_close(pixels[18], values[23], 1e-7f32);
        assert_eq!(pixels[19], inf);
        check_close(pixels[20], values[0], 1e-7f32);
        check_close(pixels[21], values[1], 1e-7f32);
        check_close(pixels[22], values[2], 1e-7f32);
        assert_eq!(pixels[23], -inf);
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, nan_half_test)` @ v2.5.2.
#[test]
fn nan_half_test() {
    let mut lut = Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();

    let values = lut.get_array_mut().get_values_mut();

    // Changed values for nan input.
    const NAN_ID_RED: usize = 32256 * 3;
    values[NAN_ID_RED] = -1.0f32;
    values[NAN_ID_RED + 1] = -2.0f32;
    values[NAN_ID_RED + 2] = -3.0f32;
    let values = values.clone();

    let renderer = get_lut1d_renderer(&lut, BitDepth::F32, BitDepth::F32).unwrap();

    let qnan = f32::NAN;
    #[rustfmt::skip]
    let mut pixels: [f32; 16] = [
        qnan, 0.5, 0.3, -0.2,
        0.5, qnan, 0.3, 0.2,
        0.5, 0.3, qnan, 1.2,
        0.5, 0.3, 0.2, qnan,
    ];

    renderer.apply(&mut pixels);

    // This verifies that a half-domain Lut1D can map NaNs to whatever the LUT author wants.
    // In this test, a different value for R, G, and B.

    check_close(pixels[0], values[NAN_ID_RED], 1e-7f32);
    check_close(pixels[5], values[NAN_ID_RED + 1], 1e-7f32);
    check_close(pixels[10], values[NAN_ID_RED + 2], 1e-7f32);
    assert!(pixels[15].is_nan());
}

/// Once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, basic)` @ v2.5.2.
#[test]
fn basic() {
    // By default, this constructor creates an 'identity LUT'.
    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 65536, false).unwrap();

    lut_data.set_file_output_bit_depth(BitDepth::F32);

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let step = 1.0f32 / (lut_data.get_array().get_length() as f32 - 1.0f32);

    #[rustfmt::skip]
    let in_img: [f32; 8] = [
        0.0, 0.0, 0.0, 1.0,
        0.0, 0.0, step, 1.0,
    ];

    let error = 1e-6f32;
    for cpu in simd_modes() {
        let cpu_op =
            get_lut1d_renderer_for_cpu(&lut_data, BitDepth::F32, BitDepth::F32, &cpu).unwrap();

        let mut out_img = vec![1.0f32; 2 * 4];
        cpu_op.apply_bit_depth(Pixels::F32(&in_img), PixelsMut::F32(&mut out_img));

        check_close(out_img[0], 0.0f32, error);
        check_close(out_img[1], 0.0f32, error);
        check_close(out_img[2], 0.0f32, error);
        check_close(out_img[3], 1.0f32, error);

        check_close(out_img[4], 0.0f32, error);
        check_close(out_img[5], 0.0f32, error);
        check_close(out_img[6], step, error);
        check_close(out_img[7], 1.0f32, error);
    }

    // No more an 'identity LUT 1D'.
    let arbitrary_val = 0.123456f32;

    lut_data.get_array_mut()[5] = arbitrary_val;

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();
    assert!(!lut_data.is_identity());
    for cpu in simd_modes() {
        let cpu_op =
            get_lut1d_renderer_for_cpu(&lut_data, BitDepth::F32, BitDepth::F32, &cpu).unwrap();

        let mut out_img = vec![1.0f32; 2 * 4];
        cpu_op.apply_bit_depth(Pixels::F32(&in_img), PixelsMut::F32(&mut out_img));

        check_close(out_img[0], 0.0f32, error);
        check_close(out_img[1], 0.0f32, error);
        check_close(out_img[2], 0.0f32, error);
        check_close(out_img[3], 1.0f32, error);

        check_close(out_img[4], 0.0f32, error);
        check_close(out_img[5], 0.0f32, error);
        check_close(out_img[6], arbitrary_val, error);
        check_close(out_img[7], 1.0f32, error);
    }
}

/// Once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, nan)` @ v2.5.2.
#[test]
fn nan() {
    // By default, this constructor creates an 'identity LUT'.
    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 65536, false).unwrap();

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    for cpu in simd_modes() {
        let cpu_op =
            get_lut1d_renderer_for_cpu(&lut_data, BitDepth::F32, BitDepth::F32, &cpu).unwrap();

        let step = 1.0f32 / (lut_data.get_array().get_length() as f32 - 1.0f32);

        #[rustfmt::skip]
    let my_image: [f32; 8] = [
        f32::NAN, 0.0, 0.0, 1.0,
        0.0, 0.0, step, 1.0,
    ];

        let mut out_img = vec![0.0f32; 2 * 4];
        cpu_op.apply_bit_depth(Pixels::F32(&my_image), PixelsMut::F32(&mut out_img));

        assert_eq!(out_img[0], 0.0f32);
        assert_eq!(out_img[1], 0.0f32);
        assert_eq!(out_img[2], 0.0f32);
        assert_eq!(out_img[3], 1.0f32);

        assert_eq!(out_img[4], 0.0f32);
        assert_eq!(out_img[5], 0.0f32);
        assert_eq!(out_img[6], step);
        assert_eq!(out_img[7], 1.0f32);
    }
}

/// Once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_red)` @ v2.5.2.
#[test]
fn lut_1d_red() {
    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 32, false).unwrap();

    let vals = lut_data.get_array_mut().get_values_mut();
    // `0.f / 1023.f, 0.f, 0.f`, `33.f / 1023.f, 0.f, 0.f`, ..., `1023.f / 1023.f, 0.f, 0.f`.
    let lut_values: Vec<f32> = (0..32)
        .flat_map(|i| [(33 * i) as f32 / 1023.0f32, 0.0, 0.0])
        .collect();
    *vals = lut_values;

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    for cpu in simd_modes() {
        let cpu_op =
            get_lut1d_renderer_for_cpu(&lut_data, BitDepth::F32, BitDepth::Uint16, &cpu).unwrap();

        let step = 1.0f32 / 31.0f32;
        #[rustfmt::skip]
    let in_img: Vec<f32> = vec![
        0.0, 0.0, 0.0, 0.0,
        step, 0.0, 0.0, 0.0,
        0.0, step, 0.0, 0.0,
        0.0, 0.0, step, 0.0,
        step, step, step, 0.0,
    ];

        let mut out_img = vec![1u16; 5 * 4];
        cpu_op.apply_bit_depth(Pixels::F32(&in_img), PixelsMut::U16(&mut out_img));

        let scaled_step = (step * 65535.0f32).round() as u16;

        assert_eq!(out_img[0], 0);
        assert_eq!(out_img[1], 0);
        assert_eq!(out_img[2], 0);
        assert_eq!(out_img[3], 0);

        assert_eq!(out_img[4], scaled_step);
        assert_eq!(out_img[5], 0);
        assert_eq!(out_img[6], 0);
        assert_eq!(out_img[7], 0);

        assert_eq!(out_img[8], 0);
        assert_eq!(out_img[9], 0);
        assert_eq!(out_img[10], 0);
        assert_eq!(out_img[11], 0);

        assert_eq!(out_img[12], 0);
        assert_eq!(out_img[13], 0);
        assert_eq!(out_img[14], 0);
        assert_eq!(out_img[15], 0);

        assert_eq!(out_img[16], scaled_step);
        assert_eq!(out_img[17], 0);
        assert_eq!(out_img[18], 0);
        assert_eq!(out_img[19], 0);
    }
}

/// The renderer of the 64k 16f identity 1D LUT from F16 to `out`, and an image of every half
/// with alpha 1.
fn identity_half_image(out: BitDepth) -> (Arc<dyn CpuOp>, Vec<half::f16>) {
    // By default, this constructor creates an 'identity lut'.
    let mut lut_data =
        Lut1DOpData::with_half_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, false).unwrap();

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&lut_data, BitDepth::F16, out).unwrap();

    const NB_PIXELS: u32 = 65536;
    let mut my_image = Vec::with_capacity(NB_PIXELS as usize * 4);
    for i in 0..NB_PIXELS {
        let h_val = half::f16::from_bits(i as u16);
        my_image.extend([h_val, h_val, h_val, half(1.0)]);
    }
    (cpu_op, my_image)
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_identity_half)` @ v2.5.2.
#[test]
fn lut_1d_identity_half() {
    let (cpu_op, my_image) = identity_half_image(BitDepth::F16);
    let mut out_img = vec![half::f16::ZERO; my_image.len()];
    cpu_op.apply_bit_depth(Pixels::F16(&my_image), PixelsMut::F16(&mut out_img));

    for (i, out) in out_img.as_chunks::<4>().0.iter().enumerate() {
        let h_val = half::f16::from_bits(i as u16);

        if h_val.is_nan() {
            assert_eq!(float(out[0]), 0.0f32);
            assert_eq!(float(out[1]), 0.0f32);
            assert_eq!(float(out[2]), 0.0f32);
            assert_eq!(float(out[3]), 1.0f32);
        } else if h_val.is_infinite() {
            assert!(out[0].is_infinite());
            assert!(out[1].is_infinite());
            assert!(out[2].is_infinite());
            assert_eq!(float(out[3]), 1.0f32);
        } else {
            assert_eq!(out[0].to_bits(), h_val.to_bits());
            assert_eq!(out[1].to_bits(), h_val.to_bits());
            assert_eq!(out[2].to_bits(), h_val.to_bits());
            assert_eq!(float(out[3]), 1.0f32);
        }
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_identity_half_to_int)` @ v2.5.2.
#[test]
fn lut_1d_identity_half_to_int() {
    let (cpu_op, my_image) = identity_half_image(BitDepth::Uint16);
    let mut out_img = vec![0u16; my_image.len()];
    cpu_op.apply_bit_depth(Pixels::F16(&my_image), PixelsMut::U16(&mut out_img));

    // `(float)OCIO::GetBitDepthMaxValue(OCIO::BIT_DEPTH_UINT16)`.
    let scale_factor = 65535.0f32;

    for (i, out) in out_img.as_chunks::<4>().0.iter().enumerate() {
        let h_val = half::f16::from_bits(i as u16);
        let f_val = scale_factor * float(h_val);

        let val = crate::math_utils::clamp(f_val + 0.5f32, 0.0f32, scale_factor) as u16;

        assert_eq!(val, out[0]);
        assert_eq!(val, out[1]);
        assert_eq!(val, out[2]);
        assert_eq!(1.0f32 * scale_factor, f32::from(out[3]));
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_identity_half_code)` @ v2.5.2.
#[test]
fn lut_1d_identity_half_code() {
    // By default, this constructor creates an 'identity lut'.
    let mut lut_data =
        Lut1DOpData::with_half_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, false).unwrap();

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&lut_data, BitDepth::F16, BitDepth::F16).unwrap();

    const NB_PIXELS: usize = 5;
    let mut my_image = vec![half::f16::ZERO; NB_PIXELS * 4];

    my_image[0] = half(0.0);
    my_image[1] = half(0.0);
    my_image[2] = half(0.0);
    my_image[3] = half(1.0);

    // Use values between points to test interpolation code.
    for i in (4..4 * NB_PIXELS).step_by(4) {
        let h_val1 = half::f16::from_bits(i as u16);
        let h_val2 = half::f16::from_bits((i + 1) as u16);
        // `fabs(hVal2 - hVal1)`: Imath's `half` converts to `float` for the subtraction.
        let delta = (float(h_val2) - float(h_val1)).abs();
        let min = if float(h_val1) < float(h_val2) {
            float(h_val1)
        } else {
            float(h_val2)
        };

        my_image[i] = half(min + (delta / i as f32));
        my_image[i + 1] = half(min + (delta / i as f32));
        my_image[i + 2] = half(min + (delta / i as f32));
        my_image[i + 3] = half(1.0);
    }

    let mut out_img = vec![half::f16::ZERO; NB_PIXELS * 4];
    cpu_op.apply_bit_depth(Pixels::F16(&my_image), PixelsMut::F16(&mut out_img));

    for i in (0..4 * NB_PIXELS).step_by(4) {
        assert_eq!(out_img[i].to_bits(), my_image[i].to_bits());
        assert_eq!(out_img[i + 1].to_bits(), my_image[i + 1].to_bits());
        assert_eq!(out_img[i + 2].to_bits(), my_image[i + 2].to_bits());
        assert_eq!(float(out_img[i + 3]), 1.0f32);
    }
}

/// A 10- or 12-bit code above the maximum is an error before the lookup (docs/improvements.md,
/// U-1): upstream reads past the table. Codes up to the maximum, and alpha, pass.
#[test]
fn codes_past_the_table_are_errors() {
    for (depth, max) in [(BitDepth::Uint10, 1023u16), (BitDepth::Uint12, 4095)] {
        let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
        let renderer = get_lut1d_renderer(&lut, depth, BitDepth::F32).unwrap();
        renderer
            .check_input(Pixels::U16(&[max, 0, max, u16::MAX]))
            .unwrap();
        for c in 0..3 {
            let mut px = [0u16; 4];
            px[c] = max + 1;
            let e = renderer.check_input(Pixels::U16(&px)).unwrap_err();
            assert_eq!(
                e.message(),
                format!(
                    "Lut1D: a {} value above {max} can't be looked up: upstream reads past the \
                     1D LUT's {} entries.",
                    crate::open_color_types::bit_depth_to_string(depth),
                    max as u32 + 1
                )
            );
        }
    }
}

/// `applyRGB` and `applyRGBA` with 10- and 12-bit input read the pixel's bytes as codes, which
/// can exceed the table (docs/improvements.md, U-1): an error, the pixel left as it was. A
/// pixel whose bytes are codes within the table is looked up.
#[test]
fn in_place_codes_past_the_table_are_errors() {
    use crate::cpu_processor::CpuProcessor;
    use crate::op::OpVec;
    use crate::open_color_types::OptimizationFlags;
    use crate::ops::lut1d::lut1d_op::create_lut1d_op;

    for depth in [BitDepth::Uint10, BitDepth::Uint12] {
        let lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
        let mut ops = OpVec::new();
        create_lut1d_op(&mut ops, lut, TransformDirection::Forward);
        ops.finalize().unwrap();
        let cpu = CpuProcessor::new(&ops, depth, BitDepth::F32, OptimizationFlags::NONE).unwrap();

        // 0.3f's low 16 bits, 0x999a, are a code past 1023 and 4095.
        let mut pixel = [0.3f32, 0.25, 0.125, 1.0];
        let e = cpu.apply_rgba(&mut pixel).unwrap_err();
        assert!(e.message().starts_with("Lut1D: a "), "{}", e.message());
        assert_eq!(pixel, [0.3f32, 0.25, 0.125, 1.0]);
        let mut rgb = [0.3f32, 0.25, 0.125];
        cpu.apply_rgb(&mut rgb).unwrap_err();
        assert_eq!(rgb, [0.3f32, 0.25, 0.125]);

        // Zero bytes are code 0 everywhere.
        let mut zero = [0.0f32; 4];
        cpu.apply_rgba(&mut zero).unwrap();
    }
}

/// The CPU processor of a forward 10- or 12-bit lookup domain (or, with `zero`, the same LUT
/// with every entry 0.0), to F32, without optimization.
fn lookup_processor(depth: BitDepth, zero: bool) -> crate::cpu_processor::CpuProcessor {
    use crate::cpu_processor::CpuProcessor;
    use crate::op::OpVec;
    use crate::open_color_types::OptimizationFlags;
    use crate::ops::lut1d::lut1d_op::create_lut1d_op;

    let mut lut = Lut1DOpData::make_lookup_domain(depth).unwrap();
    if zero {
        lut.get_array_mut().get_values_mut().fill(0.0);
    }
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut, TransformDirection::Forward);
    ops.finalize().unwrap();
    CpuProcessor::new(&ops, depth, BitDepth::F32, OptimizationFlags::NONE).unwrap()
}

/// U-1 through the image paths (docs/improvements.md): a 1x1 10-bit image whose green code is
/// past the table, RGBA (the scanline helper's packed path) and RGB (through the packer), to
/// F32, is U-1's error, not a panic.
#[test]
fn image_codes_past_the_table_are_errors() {
    use crate::image_desc::{AUTO_STRIDE, PackedImageDesc};

    let cpu = lookup_processor(BitDepth::Uint10, false);
    for channels in [4usize, 3] {
        let mut src = [100u16, 2000, 300, 1023];
        let src = PackedImageDesc::with_strides(
            &mut src[..channels],
            1,
            1,
            channels,
            BitDepth::Uint10,
            AUTO_STRIDE,
            AUTO_STRIDE,
            AUTO_STRIDE,
        )
        .unwrap();
        let mut dst = [0.0f32; 4];
        let mut dst = PackedImageDesc::new(&mut dst[..channels], 1, 1, channels).unwrap();
        let e = cpu.apply_src_dst(&src, &mut dst).unwrap_err();
        assert_eq!(
            e.message(),
            "Lut1D: a 10ui value above 1023 can't be looked up: upstream reads past the 1D LUT's \
             1024 entries.",
            "{channels} channels"
        );
    }
}

/// In place, the first code read is red's, the pixel's first 16 bits on both wheels' orders:
/// with a LUT of zeros, the floats stored read back as code 0, so a red code of exactly the
/// maximum is looked up, and one above it is U-1's error.
#[test]
fn in_place_codes_at_the_table_boundary() {
    for (depth, max) in [(BitDepth::Uint10, 1023u32), (BitDepth::Uint12, 4095)] {
        let cpu = lookup_processor(depth, true);
        let mut at_max = [f32::from_bits(max), 0.0, 0.0, 0.0];
        cpu.apply_rgba(&mut at_max).unwrap();
        let mut rgb = [f32::from_bits(max), 0.0, 0.0];
        cpu.apply_rgb(&mut rgb).unwrap();

        let past = [f32::from_bits(max + 1), 0.0, 0.0, 0.0];
        let mut pixel = past;
        let e = cpu.apply_rgba(&mut pixel).unwrap_err();
        assert!(e.message().starts_with("Lut1D: a "), "{}", e.message());
        assert_eq!(pixel.map(f32::to_bits), past.map(f32::to_bits));
        let mut rgb = [past[0], 0.0, 0.0];
        cpu.apply_rgb(&mut rgb).unwrap_err();
    }
}

/// U-1 through the scanline helper's second packed path, where the destination isn't packed
/// RGBA F32 (src/OpenColorIO/ScanlineHelper.cpp @ v2.5.2, the `m_rgbaFloatBuffer` branch): a
/// 1x1 packed RGBA 10-bit image with green 2000, to a 16-bit image, and in place through a
/// 10-bit to 10-bit processor. Both are U-1's error.
#[test]
fn codes_past_the_table_through_the_float_buffer_are_errors() {
    use crate::cpu_processor::CpuProcessor;
    use crate::image_desc::{AUTO_STRIDE, PackedImageDesc};
    use crate::op::OpVec;
    use crate::open_color_types::OptimizationFlags;
    use crate::ops::lut1d::lut1d_op::create_lut1d_op;

    let message = "Lut1D: a 10ui value above 1023 can't be looked up: upstream reads past the 1D \
                   LUT's 1024 entries.";
    let processor = |out: BitDepth| {
        let lut = Lut1DOpData::make_lookup_domain(BitDepth::Uint10).unwrap();
        let mut ops = OpVec::new();
        create_lut1d_op(&mut ops, lut, TransformDirection::Forward);
        ops.finalize().unwrap();
        CpuProcessor::new(&ops, BitDepth::Uint10, out, OptimizationFlags::NONE).unwrap()
    };
    fn packed(px: &mut [u16], depth: BitDepth) -> PackedImageDesc<&mut [u8]> {
        PackedImageDesc::with_strides(px, 1, 1, 4, depth, AUTO_STRIDE, AUTO_STRIDE, AUTO_STRIDE)
            .unwrap()
    }

    let cpu = processor(BitDepth::Uint16);
    let mut src = [100u16, 2000, 300, 1023];
    let src = packed(&mut src[..], BitDepth::Uint10);
    let mut dst = [0u16; 4];
    let mut dst = packed(&mut dst[..], BitDepth::Uint16);
    let e = cpu.apply_src_dst(&src, &mut dst).unwrap_err();
    assert_eq!(e.message(), message);

    let cpu = processor(BitDepth::Uint10);
    let mut px = [100u16, 2000, 300, 1023];
    let mut img = packed(&mut px[..], BitDepth::Uint10);
    let e = cpu.apply(&mut img).unwrap_err();
    assert_eq!(e.message(), message);
}
