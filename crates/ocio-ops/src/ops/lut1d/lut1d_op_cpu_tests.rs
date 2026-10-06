// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOpCPU_tests.cpp` @ v2.5.2, the tests that need no file
//! (`BuildOpsTest`, Phase 4); tests of the renderers' dispatch,
//! and of the 10- and 12-bit codes past the tables (docs/improvements.md, U-1). The pixels are
//! compared with the wheel's in `tests/lut1d_op_cpu_oracle.rs` and
//! `tests/lut1d_renderer_oracle.rs`.
//!
//! Upstream's test runner reruns every test in each SIMD mode the CPU supports
//! (tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2). The tests of a standard domain with float
//! input, whose renderer runs a SIMD kernel on rows of more than one pixel, run once per mode
//! here ([`simd_modes`]). The composition that resamples a LUT for a lookup, or makes an
//! inverse LUT's fast forward LUT, renders with this machine's kernel in every mode
//! (`EvalTransform` through `Op::apply`).

use super::*;
use crate::cpu_info::{
    X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512, X86_CPU_FLAG_SSE2,
};
use crate::math_utils::halfs_differ;
use crate::ops::lut1d::lut1d_op_data::{HalfFlags, make_fast_lut1d_from_inverse};
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
/// hue-adjust renderer of a standard domain with float input, the standard domain's renderer,
/// the inverse renderers, and the lookups of LUTs resampled for their input, for every output
/// bit depth.
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
    // An inverse LUT, standard or half domain, with or without hue adjust, from and to every
    // bit depth.
    let depths = [
        BitDepth::Uint8,
        BitDepth::Uint10,
        BitDepth::Uint12,
        BitDepth::Uint16,
        BitDepth::F16,
        BitDepth::F32,
    ];
    for hue_adjust in [Lut1DHueAdjust::None, Lut1DHueAdjust::Dw3] {
        for lut in [&lut8, &half_lut] {
            let mut inverse = lut.inverse();
            inverse.set_hue_adjust(hue_adjust).unwrap();
            inverse.finalize().unwrap();
            for (inp, out) in depths.iter().flat_map(|&i| depths.map(|o| (i, o))) {
                get_lut1d_renderer(&inverse, inp, out).unwrap();
            }
        }
    }
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
    // LUTs the lookup must resample first.
    get_lut1d_renderer(&hue, BitDepth::Uint10, BitDepth::F32).unwrap();
    get_lut1d_renderer(&lut8, BitDepth::Uint10, BitDepth::F32).unwrap();
    get_lut1d_renderer(&half_lut, BitDepth::Uint16, BitDepth::F32).unwrap();
    // The scalar profile is the standard domain's.
    for lut in [&half_lut, &lut8.inverse(), &hue] {
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

/// The LUT of `bit_depth_support`: "Copy & paste of logtolin_8to8.lut"
/// (tests/cpu/ops/lut1d/Lut1DOpCPU_tests.cpp:235-491 @ v2.5.2).
#[rustfmt::skip]
const LOGTOLIN_8TO8: [f32; 768] = [
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    0.0, 0.0, 0.0,
    1.0, 1.0, 1.0,
    1.0, 1.0, 1.0,
    2.0, 2.0, 2.0,
    2.0, 2.0, 2.0,
    3.0, 3.0, 3.0,
    3.0, 3.0, 3.0,
    4.0, 4.0, 4.0,
    5.0, 5.0, 5.0,
    5.0, 5.0, 5.0,
    6.0, 6.0, 6.0,
    6.0, 6.0, 6.0,
    7.0, 7.0, 7.0,
    8.0, 8.0, 8.0,
    8.0, 8.0, 8.0,
    9.0, 9.0, 9.0,
    10.0, 10.0, 10.0,
    10.0, 10.0, 10.0,
    11.0, 11.0, 11.0,
    12.0, 12.0, 12.0,
    12.0, 12.0, 12.0,
    13.0, 13.0, 13.0,
    14.0, 14.0, 14.0,
    15.0, 15.0, 15.0,
    15.0, 15.0, 15.0,
    16.0, 16.0, 16.0,
    17.0, 17.0, 17.0,
    18.0, 18.0, 18.0,
    18.0, 18.0, 18.0,
    19.0, 19.0, 19.0,
    20.0, 20.0, 20.0,
    21.0, 21.0, 21.0,
    22.0, 22.0, 22.0,
    22.0, 22.0, 22.0,
    23.0, 23.0, 23.0,
    24.0, 24.0, 24.0,
    25.0, 25.0, 25.0,
    26.0, 26.0, 26.0,
    27.0, 27.0, 27.0,
    28.0, 28.0, 28.0,
    29.0, 29.0, 29.0,
    30.0, 30.0, 30.0,
    30.0, 30.0, 30.0,
    31.0, 31.0, 31.0,
    32.0, 32.0, 32.0,
    33.0, 33.0, 33.0,
    34.0, 34.0, 34.0,
    35.0, 35.0, 35.0,
    36.0, 36.0, 36.0,
    37.0, 37.0, 37.0,
    39.0, 39.0, 39.0,
    40.0, 40.0, 40.0,
    41.0, 41.0, 41.0,
    42.0, 42.0, 42.0,
    43.0, 43.0, 43.0,
    44.0, 44.0, 44.0,
    45.0, 45.0, 45.0,
    46.0, 46.0, 46.0,
    48.0, 48.0, 48.0,
    49.0, 49.0, 49.0,
    50.0, 50.0, 50.0,
    51.0, 51.0, 51.0,
    52.0, 52.0, 52.0,
    54.0, 54.0, 54.0,
    55.0, 55.0, 55.0,
    56.0, 56.0, 56.0,
    58.0, 58.0, 58.0,
    59.0, 59.0, 59.0,
    60.0, 60.0, 60.0,
    62.0, 62.0, 62.0,
    63.0, 63.0, 63.0,
    64.0, 64.0, 64.0,
    66.0, 66.0, 66.0,
    67.0, 67.0, 67.0,
    69.0, 69.0, 69.0,
    70.0, 70.0, 70.0,
    72.0, 72.0, 72.0,
    73.0, 73.0, 73.0,
    75.0, 75.0, 75.0,
    76.0, 76.0, 76.0,
    78.0, 78.0, 78.0,
    80.0, 80.0, 80.0,
    81.0, 81.0, 81.0,
    83.0, 83.0, 83.0,
    85.0, 85.0, 85.0,
    86.0, 86.0, 86.0,
    88.0, 88.0, 88.0,
    90.0, 90.0, 90.0,
    92.0, 92.0, 92.0,
    94.0, 94.0, 94.0,
    95.0, 95.0, 95.0,
    97.0, 97.0, 97.0,
    99.0, 99.0, 99.0,
    101.0, 101.0, 101.0,
    103.0, 103.0, 103.0,
    105.0, 105.0, 105.0,
    107.0, 107.0, 107.0,
    109.0, 109.0, 109.0,
    111.0, 111.0, 111.0,
    113.0, 113.0, 113.0,
    115.0, 115.0, 115.0,
    117.0, 117.0, 117.0,
    120.0, 120.0, 120.0,
    122.0, 122.0, 122.0,
    124.0, 124.0, 124.0,
    126.0, 126.0, 126.0,
    129.0, 129.0, 129.0,
    131.0, 131.0, 131.0,
    133.0, 133.0, 133.0,
    136.0, 136.0, 136.0,
    138.0, 138.0, 138.0,
    140.0, 140.0, 140.0,
    143.0, 143.0, 143.0,
    145.0, 145.0, 145.0,
    148.0, 148.0, 148.0,
    151.0, 151.0, 151.0,
    153.0, 153.0, 153.0,
    156.0, 156.0, 156.0,
    159.0, 159.0, 159.0,
    161.0, 161.0, 161.0,
    164.0, 164.0, 164.0,
    167.0, 167.0, 167.0,
    170.0, 170.0, 170.0,
    173.0, 173.0, 173.0,
    176.0, 176.0, 176.0,
    179.0, 179.0, 179.0,
    182.0, 182.0, 182.0,
    185.0, 185.0, 185.0,
    188.0, 188.0, 188.0,
    191.0, 191.0, 191.0,
    194.0, 194.0, 194.0,
    198.0, 198.0, 198.0,
    201.0, 201.0, 201.0,
    204.0, 204.0, 204.0,
    208.0, 208.0, 208.0,
    211.0, 211.0, 211.0,
    214.0, 214.0, 214.0,
    218.0, 218.0, 218.0,
    222.0, 222.0, 222.0,
    225.0, 225.0, 225.0,
    229.0, 229.0, 229.0,
    233.0, 233.0, 233.0,
    236.0, 236.0, 236.0,
    240.0, 240.0, 240.0,
    244.0, 244.0, 244.0,
    248.0, 248.0, 248.0,
    252.0, 252.0, 252.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
    255.0, 255.0, 255.0,
];

/// `FastFromInverse` (tests/cpu/ops/lut1d/Lut1DOpCPU_tests.cpp:213-222 @ v2.5.2): validates
/// and finalizes the inverse LUT, and returns its fast forward LUT.
fn fast_from_inverse(inv_lut_data: &mut Lut1DOpData) -> Lut1DOpData {
    inv_lut_data.validate().unwrap();
    inv_lut_data.finalize().unwrap();
    make_fast_lut1d_from_inverse(inv_lut_data).unwrap()
}

/// `BaseLut1DRenderer::isLookup()`: whether the renderer looks the LUT up with the codes of
/// its integer or half input.
fn is_lookup(op: &Arc<dyn CpuOp>) -> bool {
    let name = format!("{op:?}");
    name.starts_with("Lut1DLookupRenderer") || name.starts_with("Lut1DHueAdjustLookupRenderer")
}

/// The float renderers of a standard domain run once per SIMD mode (module docs); the
/// lookups, which a SIMD mode doesn't change but for the resampling of `FastFromInverse`'s
/// LUT, which renders with this machine's kernel, once.
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, bit_depth_support)` @ v2.5.2.
#[test]
fn bit_depth_support() {
    // Unit test to validate the pixel bit depth processing with the 1D LUT.

    // Note: Copy & paste of logtolin_8to8.lut

    let mut lut_data = Lut1DOpData::new(256).unwrap();

    *lut_data.get_array_mut().get_values_mut() = LOGTOLIN_8TO8.to_vec();
    lut_data.get_array_mut().scale(1.0f32 / 255.0f32);
    let const_lut = &lut_data;

    const NB_PIXELS: usize = 4;

    #[rustfmt::skip]
    let uint8_in_img: [u8; NB_PIXELS * 4] = [
          0,   1,   2,   0,
         50,  51,  52, 255,
        150, 151, 152,   0,
        230, 240, 250, 255,
    ];

    #[rustfmt::skip]
    let uint16_out_img: [u16; NB_PIXELS * 4] = [
            0,     0,     0,     0,
         4369,  4626,  4626, 65535,
        46774, 47545, 48316,     0,
        65535, 65535, 65535, 65535,
    ];

    // Processing from UINT8 to UINT8.
    {
        let cpu_op = get_lut1d_renderer(const_lut, BitDepth::Uint8, BitDepth::Uint8).unwrap();
        assert!(is_lookup(&cpu_op));

        let mut out_img = vec![0u8; NB_PIXELS * 4];

        cpu_op.apply_bit_depth(Pixels::U8(&uint8_in_img), PixelsMut::U8(&mut out_img));

        #[rustfmt::skip]
        assert_eq!(out_img, [
              0,   0,   0,   0,
             17,  18,  18, 255,
            182, 185, 188,   0,
            255, 255, 255, 255,
        ]);
    }

    // Processing from UINT8 to UINT8, using the inverse LUT.
    {
        let mut lut_inv_data = lut_data.inverse();
        let const_inv_lut = fast_from_inverse(&mut lut_inv_data);

        let cpu_op = get_lut1d_renderer(&const_inv_lut, BitDepth::Uint8, BitDepth::Uint8).unwrap();
        assert!(is_lookup(&cpu_op));

        let mut out_img = vec![0u8; NB_PIXELS * 4];

        cpu_op.apply_bit_depth(Pixels::U8(&uint8_in_img), PixelsMut::U8(&mut out_img));

        #[rustfmt::skip]
        assert_eq!(out_img, [
             24,  25,  27,   0,
             84,  85,  86, 255,
            139, 139, 140,   0,
            164, 167, 170, 255,
        ]);
    }

    // Processing from UINT8 to UINT16.
    {
        let cpu_op = get_lut1d_renderer(const_lut, BitDepth::Uint8, BitDepth::Uint16).unwrap();
        assert!(is_lookup(&cpu_op));

        let mut out_img = vec![0u16; NB_PIXELS * 4];

        cpu_op.apply_bit_depth(Pixels::U8(&uint8_in_img), PixelsMut::U16(&mut out_img));

        for i in 0..NB_PIXELS * 4 {
            assert_eq!(out_img[i], uint16_out_img[i]);
        }
    }

    // Processing from UINT8 to F16.
    {
        let cpu_op = get_lut1d_renderer(const_lut, BitDepth::Uint8, BitDepth::F16).unwrap();
        assert!(is_lookup(&cpu_op));

        let mut out_img = vec![half(0.0); NB_PIXELS * 4];

        cpu_op.apply_bit_depth(Pixels::U8(&uint8_in_img), PixelsMut::F16(&mut out_img));

        assert_eq!(float(out_img[0]), 0.0f32);
        assert_eq!(float(out_img[1]), 0.0f32);
        assert_eq!(float(out_img[2]), 0.0f32);
        assert_eq!(float(out_img[3]), 0.0f32);

        check_close(float(out_img[4]), 0.066650390625f32, 1e-6f32);
        check_close(float(out_img[5]), 0.070617675781f32, 1e-6f32);
        check_close(float(out_img[6]), 0.070617675781f32, 1e-6f32);
        assert_eq!(float(out_img[7]), 1.0f32);

        check_close(float(out_img[8]), 0.7138671875f32, 1e-6f32);
        check_close(float(out_img[9]), 0.7255859375f32, 1e-6f32);
        check_close(float(out_img[10]), 0.7373046875f32, 1e-6f32);
        assert_eq!(float(out_img[11]), 0.0f32);

        assert_eq!(float(out_img[12]), 1.0f32);
        assert_eq!(float(out_img[13]), 1.0f32);
        assert_eq!(float(out_img[14]), 1.0f32);
        assert_eq!(float(out_img[15]), 1.0f32);
    }

    // Processing from UINT8 to F32.
    {
        let cpu_op = get_lut1d_renderer(const_lut, BitDepth::Uint8, BitDepth::F32).unwrap();
        assert!(is_lookup(&cpu_op));

        let mut out_img = vec![0.0f32; NB_PIXELS * 4];

        cpu_op.apply_bit_depth(Pixels::U8(&uint8_in_img), PixelsMut::F32(&mut out_img));

        assert_eq!(out_img[0], 0.0f32);
        assert_eq!(out_img[1], 0.0f32);
        assert_eq!(out_img[2], 0.0f32);
        assert_eq!(out_img[3], 0.0f32);

        check_close(out_img[4], 0.06666666666666667f32, 1e-6f32);
        check_close(out_img[5], 0.07058823529411765f32, 1e-6f32);
        check_close(out_img[6], 0.07058823529411765f32, 1e-6f32);
        assert_eq!(out_img[7], 1.0f32);

        check_close(out_img[8], 0.7137254901960784f32, 1e-6f32);
        check_close(out_img[9], 0.7254901960784313f32, 1e-6f32);
        check_close(out_img[10], 0.7372549019607844f32, 1e-6f32);
        assert_eq!(out_img[11], 0.0f32);

        assert_eq!(out_img[12], 1.0f32);
        assert_eq!(out_img[13], 1.0f32);
        assert_eq!(out_img[14], 1.0f32);
        assert_eq!(out_img[15], 1.0f32);
    }

    // Use scaled previous input values so previous output values could be
    // reused (i.e. uint16_outImg) to validate the pixel bit depth processing.

    let float_in_img: Vec<f32> = uint8_in_img
        .iter()
        .map(|&v| f32::from(v) / 255.0f32)
        .collect();

    for cpu in simd_modes() {
        // LUT will be used for interpolation, not look-up.
        {
            let cpu_op =
                get_lut1d_renderer_for_cpu(const_lut, BitDepth::F32, BitDepth::Uint8, &cpu)
                    .unwrap();
            assert!(!is_lookup(&cpu_op));

            let mut out_img = vec![0u8; NB_PIXELS * 4];

            cpu_op.apply_bit_depth(Pixels::F32(&float_in_img), PixelsMut::U8(&mut out_img));

            #[rustfmt::skip]
            assert_eq!(out_img, [
                  0,   0,   0,   0,
                 17,  18,  18, 255,
                182, 185, 188,   0,
                255, 255, 255, 255,
            ]);
        }

        // LUT will be used for interpolation, not look-up.
        {
            let cpu_op =
                get_lut1d_renderer_for_cpu(const_lut, BitDepth::F32, BitDepth::Uint16, &cpu)
                    .unwrap();
            assert!(!is_lookup(&cpu_op));

            let mut out_img = vec![0u16; NB_PIXELS * 4];

            cpu_op.apply_bit_depth(Pixels::F32(&float_in_img), PixelsMut::U16(&mut out_img));

            for i in 0..NB_PIXELS * 4 {
                assert_eq!(out_img[i], uint16_out_img[i]);
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, half)` @ v2.5.2.
#[test]
fn half_test() {
    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 65536, false).unwrap();

    let step = 1.0f32 / (lut_data.get_array().get_length() as f32 - 1.0f32);

    // No more an 'identity LUT 1D'.
    const ARBITRARY_VAL: f32 = 0.123456f32;
    lut_data.get_array_mut()[5] = ARBITRARY_VAL;
    assert!(!lut_data.is_identity());

    #[rustfmt::skip]
    let in_img: [half::f16; 8] = [
        half(0.1), half(0.3), half(0.4), half(1.0),
        half(0.0), half(0.9), half(step), half(0.0),
    ];

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&lut_data, BitDepth::F16, BitDepth::F32).unwrap();

    let mut out_img = vec![-1.0f32; 2 * 4];
    cpu_op.apply_bit_depth(Pixels::F16(&in_img), PixelsMut::F32(&mut out_img));

    assert_eq!(out_img[0], float(in_img[0]));
    assert_eq!(out_img[1], float(in_img[1]));
    assert_eq!(out_img[2], float(in_img[2]));
    assert_eq!(out_img[3], float(in_img[3]));

    assert_eq!(out_img[4], float(in_img[4]));
    assert_eq!(out_img[5], float(in_img[5]));
    check_close(out_img[6], ARBITRARY_VAL, 1e-5f32);
    assert_eq!(out_img[7], float(in_img[7]));
}

/// `0.f, 0.f / 1023.f, ...`: the values of a 32-entry LUT that ramps one channel from 0 to 1
/// in steps of 33 / 1023 and leaves the others at 0 (`lut_1d_green`, `lut_1d_blue`).
fn one_channel_ramp(channel: usize) -> Vec<f32> {
    (0..32)
        .flat_map(|i| {
            let mut rgb = [0.0f32; 3];
            rgb[channel] = (33 * i) as f32 / 1023.0f32;
            rgb
        })
        .collect()
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_green)` @ v2.5.2.
#[test]
fn lut_1d_green() {
    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 32, false).unwrap();

    // `0.f, 0.f / 1023.f, 0.f`, `0.f, 33.f / 1023.f, 0.f`, ..., `0.f, 1023.f / 1023.f, 0.f`.
    *lut_data.get_array_mut().get_values_mut() = one_channel_ramp(1);

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&lut_data, BitDepth::Uint16, BitDepth::F32).unwrap();

    const STEP: u16 = (65535 / 31) as u16;
    #[rustfmt::skip]
    let uint16_in_img: [u16; 20] = [
        0,    0,    0,    0,
        STEP, 0,    0,    0,
        0,    STEP, 0,    0,
        0,    0,    STEP, 0,
        STEP, STEP, STEP, 0,
    ];

    let mut out_img = vec![-1.0f32; 5 * 4];
    cpu_op.apply_bit_depth(Pixels::U16(&uint16_in_img), PixelsMut::F32(&mut out_img));

    let scaled_step = STEP as f32 / 65535.0f32;

    assert_eq!(out_img[0], 0.0f32);
    assert_eq!(out_img[1], 0.0f32);
    assert_eq!(out_img[2], 0.0f32);
    assert_eq!(out_img[3], 0.0f32);

    assert_eq!(out_img[4], 0.0f32);
    assert_eq!(out_img[5], 0.0f32);
    assert_eq!(out_img[6], 0.0f32);
    assert_eq!(out_img[7], 0.0f32);

    assert_eq!(out_img[8], 0.0f32);
    assert_eq!(out_img[9], scaled_step);
    assert_eq!(out_img[10], 0.0f32);
    assert_eq!(out_img[11], 0.0f32);

    assert_eq!(out_img[12], 0.0f32);
    assert_eq!(out_img[13], 0.0f32);
    assert_eq!(out_img[14], 0.0f32);
    assert_eq!(out_img[15], 0.0f32);

    assert_eq!(out_img[16], 0.0f32);
    assert_eq!(out_img[17], scaled_step);
    assert_eq!(out_img[18], 0.0f32);
    assert_eq!(out_img[19], 0.0f32);
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_blue)` @ v2.5.2.
#[test]
fn lut_1d_blue() {
    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, 32, false).unwrap();

    // `0.f, 0.f, 0.f / 1023.f`, `0.f, 0.f, 33.f / 1023.f`, ..., `0.f, 0.f, 1023.f / 1023.f`.
    *lut_data.get_array_mut().get_values_mut() = one_channel_ramp(2);

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&lut_data, BitDepth::Uint16, BitDepth::Uint16).unwrap();

    const STEP: u16 = (65535 / 31) as u16;
    #[rustfmt::skip]
    let uint16_in_img: [u16; 20] = [
        0,    0,    0,    0,
        STEP, 0,    0,    0,
        0,    STEP, 0,    0,
        0,    0,    STEP, 0,
        STEP, STEP, STEP, 0,
    ];

    let mut out_img = vec![2000u16; 5 * 4];
    cpu_op.apply_bit_depth(Pixels::U16(&uint16_in_img), PixelsMut::U16(&mut out_img));

    assert_eq!(out_img[0], 0);
    assert_eq!(out_img[1], 0);
    assert_eq!(out_img[2], 0);
    assert_eq!(out_img[3], 0);

    assert_eq!(out_img[4], 0);
    assert_eq!(out_img[5], 0);
    assert_eq!(out_img[6], 0);
    assert_eq!(out_img[7], 0);

    assert_eq!(out_img[8], 0);
    assert_eq!(out_img[9], 0);
    assert_eq!(out_img[10], 0);
    assert_eq!(out_img[11], 0);

    assert_eq!(out_img[12], 0);
    assert_eq!(out_img[13], 0);
    assert_eq!(out_img[14], STEP);
    assert_eq!(out_img[15], 0);

    assert_eq!(out_img[16], 0);
    assert_eq!(out_img[17], 0);
    assert_eq!(out_img[18], STEP);
    assert_eq!(out_img[19], 0);
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_identity_int_to_half)` @ v2.5.2.
#[test]
fn lut_1d_identity_int_to_half() {
    // Create the 64k 16f Identity 1D LUT and the test Image.

    // By default, this constructor creates an 'identity lut'.
    let mut lut_data =
        Lut1DOpData::with_half_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, false).unwrap();

    lut_data.validate().unwrap();
    lut_data.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&lut_data, BitDepth::Uint16, BitDepth::F16).unwrap();

    const NB_PIXELS: u32 = 65536;
    let mut my_image = Vec::with_capacity(NB_PIXELS as usize * 4);

    for i in 0..NB_PIXELS {
        my_image.extend([i as u16, i as u16, i as u16, 1]);
    }

    let mut out_img = vec![half(0.0); NB_PIXELS as usize * 4];
    cpu_op.apply_bit_depth(Pixels::U16(&my_image), PixelsMut::F16(&mut out_img));

    let scale_factor = 1.0f32 / Uint16::MAX_VALUE as f32;
    let h_scale_factor = half(scale_factor);
    const TOL: i32 = 1;
    for i in 0..NB_PIXELS as usize {
        let img_cntr = i * 4;
        let h_val = half(scale_factor * i as f32);

        assert!(!halfs_differ(out_img[img_cntr], h_val, TOL));
        assert!(!halfs_differ(out_img[img_cntr + 1], h_val, TOL));
        assert!(!halfs_differ(out_img[img_cntr + 2], h_val, TOL));
        assert_eq!(out_img[img_cntr + 3].to_bits(), h_scale_factor.to_bits());
    }
}

/// A LUT whose three channels are `values[i]` at entry `i`, as upstream's tests set them
/// (`vals[i] = vals[i+1] = vals[i+2] = ...`).
fn gray_lut(values: &[f32], file_depth: BitDepth) -> Lut1DOpData {
    let mut lut_data = Lut1DOpData::new(values.len() as _).unwrap();
    lut_data.set_file_output_bit_depth(file_depth);
    let vals = lut_data.get_array_mut().get_values_mut();
    for (i, &v) in values.iter().enumerate() {
        vals[i * 3] = v;
        vals[i * 3 + 1] = v;
        vals[i * 3 + 2] = v;
    }
    lut_data
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_identity)` @ v2.5.2.
#[test]
fn lut_1d_inv_identity() {
    // By default, this constructor creates an 'identity lut'.
    let dim = Lut1DOpData::get_lut_ideal_size(BitDepth::Uint10).unwrap();

    let mut lut_data = Lut1DOpData::with_half_flags(HalfFlags::STANDARD, dim, false).unwrap();

    lut_data.set_file_output_bit_depth(BitDepth::Uint10);

    let mut inv_lut = lut_data.inverse();
    let const_lut = fast_from_inverse(&mut inv_lut);

    let cpu_op = get_lut1d_renderer(&const_lut, BitDepth::Uint10, BitDepth::F32).unwrap();

    const STEPUI: u16 = 700; // relative to 10i.
    let step = STEPUI as f32 / 1023.0f32;

    #[rustfmt::skip]
    let in_image: [u16; 20] = [
        0,      0,      0,      0,
        STEPUI, 0,      0,      0,
        0,      STEPUI, 0,      0,
        0,      0,      STEPUI, 0,
        STEPUI, STEPUI, STEPUI, 0,
    ];

    let mut out_image = [-1.0f32; 20];

    // Inverse of identity should still be identity.
    #[rustfmt::skip]
    let expected: [f32; 20] = [
        0.0,  0.0,  0.0,  0.0,
        step, 0.0,  0.0,  0.0,
        0.0,  step, 0.0,  0.0,
        0.0,  0.0,  step, 0.0,
        step, step, step, 0.0,
    ];

    cpu_op.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::F32(&mut out_image));

    for i in 0..20 {
        check_close(out_image[i], expected[i], 1e-6f32);
    }

    // Repeat with EXACT.
    let const_lut = inv_lut;
    let cpu_op_exact = get_lut1d_renderer(&const_lut, BitDepth::Uint10, BitDepth::F32).unwrap();

    cpu_op_exact.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::F32(&mut out_image));

    for i in 0..20 {
        check_close(out_image[i], expected[i], 1e-6f32);
    }
}

/// The renderers of a standard domain from F32 input run once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_increasing)` @ v2.5.2.
#[test]
fn lut_1d_inv_increasing() {
    // This is a typical "easy" lut with a simple power function.
    // Linear to 1/2.2 gamma corrected code values.
    let codes: [f32; 32] = [
        0.0, 215.0, 294.0, 354.0, 403.0, 446.0, 485.0, 520.0, 553.0, 583.0, 612.0, 639.0, 665.0,
        689.0, 713.0, 735.0, 757.0, 779.0, 799.0, 819.0, 838.0, 857.0, 875.0, 893.0, 911.0, 928.0,
        944.0, 961.0, 977.0, 992.0, 1008.0, 1023.0,
    ];
    let values: Vec<f32> = codes.iter().map(|c| c / 1023.0f32).collect();
    let lut_data = gray_lut(&values, BitDepth::Uint10);

    let mut inv_lut = lut_data.inverse();

    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::Uint10, BitDepth::Uint16).unwrap();

    // The first 2 rows are actual LUT entries, the others are intermediate values.
    #[rustfmt::skip]
    let in_image: [u16; 20] = [ // scaled to 10i
          0,  215,  446,    0,
        639,  944, 1023,  445, // also test alpha
         40,  190,  260,  685,
        380,  540,  767, 1023,
        888, 1000, 1018,    0,
    ];

    let mut out_image = [u16::MAX; 20];

    #[rustfmt::skip]
    let expected: [u16; 20] = [ // scaled to 16i
            0,  2114, 10570,     0,
        23254, 54965, 65535, 28507,
          393,  1868,  3318, 43882,
         7464, 16079, 34785, 65535,
        48036, 62364, 64830,     0,
    ];

    cpu_op.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::U16(&mut out_image));

    for i in 0..20 {
        assert_eq!(out_image[i], expected[i]);
    }

    // Repeat with FAST.
    let const_inv_lut = fast_from_inverse(&mut inv_lut);
    let cpu_op_fast =
        get_lut1d_renderer(&const_inv_lut, BitDepth::Uint10, BitDepth::Uint16).unwrap();

    cpu_op_fast.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::U16(&mut out_image));

    for i in 0..20 {
        assert_eq!(out_image[i], expected[i]);
    }
}

/// The fast LUT's renderer from F32 input runs once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_decreasing_reversals)` @ v2.5.2.
#[test]
fn lut_1d_inv_decreasing_reversals() {
    // This is a more "difficult" LUT that is decreasing and has reversals
    // and values outside the typical range.
    let codes: [f32; 12] = [
        90.0, 90.0, 100.0, 80.0, 70.0, 50.0, 60.0, 70.0, 40.0, 20.0,
        -10.0, // note: LUT vals may exceed [0,255]
        -10.0,
    ];
    let values: Vec<f32> = codes.iter().map(|c| c / 255.0f32).collect();
    let lut_data = gray_lut(&values, BitDepth::Uint8);

    let mut inv_lut = lut_data.inverse();

    // Render as 32f in depth so we can test negative input vals.
    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();

    // Default InvStyle should be 'FAST' but we test the 'EXACT' InvStyle first.
    let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::F32, BitDepth::Uint16).unwrap();

    // Render as 32f in depth so we can test negative input vals.
    let in_scale_factor = 1.0f32 / Uint8::MAX_VALUE as f32;

    #[rustfmt::skip]
    let in_image: [f32; 16] = [ // scaled to 32f
        100.0 * in_scale_factor, 90.0 * in_scale_factor,  85.0 * in_scale_factor, 0.0,
         75.0 * in_scale_factor, 60.0 * in_scale_factor,  50.0 * in_scale_factor, 0.0,
         45.0 * in_scale_factor, 30.0 * in_scale_factor, -10.0 * in_scale_factor, 0.0,
        -20.0 * in_scale_factor, 75.0 * in_scale_factor,  30.0 * in_scale_factor, 0.0,
    ];

    let mut out_image = [u16::MAX; 16];

    #[rustfmt::skip]
    let mut expected: [u16; 16] = [ // scaled to 16i
        11915, 11915, 14894, 0,
        20852, 26810, 29789, 0,
        44683, 50641, 59577, 0,
        59577, 20852, 50641, 0,
    ];

    cpu_op.apply_bit_depth(Pixels::F32(&in_image), PixelsMut::U16(&mut out_image));

    for i in 0..16 {
        assert_eq!(out_image[i], expected[i]);
    }

    // Repeat with FAST.
    let const_inv_lut = fast_from_inverse(&mut inv_lut);

    // Note: When there are flat spots in the original LUT, the approximate
    // inverse LUT used in FAST mode has vertical jumps and so one would expect
    // significant differences from EXACT mode (which returns the left edge).
    // Since any value that is within the flat spot would result in the original
    // value on forward interpolation, we may loosen the tolerance for the inverse
    // to the domain of the flat spot.  Also, note that this is only an issue for
    // 32f inDepths since in all other cases EXACT mode is used to compute a LUT
    // that is used for look-up rather than interpolation.
    expected[1] = 11924;
    expected[6] = 38433;

    for cpu in simd_modes() {
        let cpu_op_fast =
            get_lut1d_renderer_for_cpu(&const_inv_lut, BitDepth::F32, BitDepth::Uint16, &cpu)
                .unwrap();

        cpu_op_fast.apply_bit_depth(Pixels::F32(&in_image), PixelsMut::U16(&mut out_image));

        for i in 0..16 {
            assert_eq!(out_image[i], expected[i]);
        }
    }
}

/// The fast LUT's renderer from F32 input runs once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_clamp_to_range)` @ v2.5.2.
#[test]
fn lut_1d_inv_clamp_to_range() {
    // Note that the start and end values do not span the full [0,255] range
    // so we test that input values are clamped correctly to this range when
    // the LUT has no flat spots at start or end.
    let codes: [f32; 12] = [
        30.0, 40.0, 60.0, 65.0, 70.0, 50.0, 60.0, 70.0, 100.0, 190.0, 200.0, 210.0,
    ];
    let values: Vec<f32> = codes.iter().map(|c| c / 255.0f32).collect();
    let lut_data = gray_lut(&values, BitDepth::Uint8);

    let mut inv_lut = lut_data.inverse();

    // Render as 32f in depth so we can test negative input vals.
    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::F32, BitDepth::Uint16).unwrap();

    let in_scale_factor = 1.0f32 / Uint8::MAX_VALUE as f32;

    #[rustfmt::skip]
    let in_image: [f32; 12] = [ // scaled to 32f
          0.0 * in_scale_factor,  10.0 * in_scale_factor,  30.0 * in_scale_factor, 0.0,
         35.0 * in_scale_factor, 202.0 * in_scale_factor, 210.0 * in_scale_factor, 0.0,
        -10.0 * in_scale_factor, 255.0 * in_scale_factor, 355.0 * in_scale_factor, 0.0,
    ];

    let mut out_image = [u16::MAX; 12];

    #[rustfmt::skip]
    let expected: [u16; 12] = [ // scaled to 16i
            0,     0,     0, 0,
         2979, 60769, 65535, 0,
            0, 65535, 65535, 0,
    ];

    cpu_op.apply_bit_depth(Pixels::F32(&in_image), PixelsMut::U16(&mut out_image));

    for i in 0..12 {
        assert_eq!(out_image[i], expected[i]);
    }

    // Repeat with FAST.
    let const_inv_lut = fast_from_inverse(&mut inv_lut);
    for cpu in simd_modes() {
        let cpu_op_fast =
            get_lut1d_renderer_for_cpu(&const_inv_lut, BitDepth::F32, BitDepth::Uint16, &cpu)
                .unwrap();

        cpu_op_fast.apply_bit_depth(Pixels::F32(&in_image), PixelsMut::U16(&mut out_image));

        for i in 0..12 {
            assert_eq!(out_image[i], expected[i]);
        }
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_flat_start_or_end)` @ v2.5.2.
#[test]
fn lut_1d_inv_flat_start_or_end() {
    let mut lut_data = Lut1DOpData::new(9).unwrap();
    lut_data.set_file_output_bit_depth(BitDepth::Uint10);

    // This LUT tests that flat spots at beginning and end of various lengths
    // are handled for increasing and decreasing LUTs (it also verifies that
    // LUTs with different R, G, B values are handled correctly).
    #[rustfmt::skip]
    let lut_values: [f32; 27] = [ // scaled to 32f
        900.0 / 1023.0,  70.0 / 1023.0,  70.0 / 1023.0,
        900.0 / 1023.0,  70.0 / 1023.0, 120.0 / 1023.0,
        900.0 / 1023.0, 120.0 / 1023.0, 300.0 / 1023.0,
        900.0 / 1023.0, 300.0 / 1023.0, 450.0 / 1023.0,
        450.0 / 1023.0, 450.0 / 1023.0, 900.0 / 1023.0,
        300.0 / 1023.0, 900.0 / 1023.0, 900.0 / 1023.0,
        120.0 / 1023.0, 900.0 / 1023.0, 900.0 / 1023.0,
         70.0 / 1023.0, 900.0 / 1023.0, 900.0 / 1023.0,
         70.0 / 1023.0, 900.0 / 1023.0, 900.0 / 1023.0,
    ];

    *lut_data.get_array_mut().get_values_mut() = lut_values.to_vec();

    let mut inv_lut = lut_data.inverse();

    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::Uint10, BitDepth::Uint16).unwrap();

    #[rustfmt::skip]
    let in_image: [u16; 48] = [ // scaled to 10i
        1023, 1023, 1023, 0,
         900,  900,  900, 0,
         800,  800,  800, 0,
         500,  500,  500, 0,
         450,  450,  450, 0,
         330,  330,  330, 0,
         150,  150,  150, 0,
         120,  120,  120, 0,
          80,   80,   80, 0,
          70,   70,   70, 0,
          60,   60,   60, 0,
           0,    0,    0, 0,
    ];

    let mut out_image = [u16::MAX; 48];

    #[rustfmt::skip]
    let expected: [u16; 48] = [ // scaled to 16i
        24576, 40959, 32768, 0,
        24576, 40959, 32768, 0,
        26396, 39139, 30947, 0,
        31857, 33678, 25486, 0,
        32768, 32768, 24576, 0,
        39321, 26214, 18022, 0,
        47786, 17749,  9557, 0,
        49151, 16384,  8192, 0,
        55705,  9830,  1638, 0,
        57343,  8192,     0, 0,
        57343,  8192,     0, 0,
        57343,  8192,     0, 0,
    ];

    cpu_op.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::U16(&mut out_image));

    for i in 0..12 * 4 {
        assert_eq!(out_image[i], expected[i]);
    }

    // Repeat with FAST.
    let const_inv_lut = fast_from_inverse(&mut inv_lut);
    let cpu_op_fast =
        get_lut1d_renderer(&const_inv_lut, BitDepth::Uint10, BitDepth::Uint16).unwrap();

    cpu_op_fast.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::U16(&mut out_image));

    for i in 0..12 * 4 {
        assert_eq!(out_image[i], expected[i]);
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_half_input)` @ v2.5.2.
#[test]
fn lut_1d_inv_half_input() {
    const DIM: usize = 15;
    let mut lut_data = Lut1DOpData::new(DIM as _).unwrap();
    lut_data.set_file_output_bit_depth(BitDepth::Uint8);

    // LUT entries.
    #[rustfmt::skip]
    let lut_entries: [f32; DIM] = [
        0.00, 0.05, 0.10, 0.15, 0.20,
        0.30, 0.40, 0.50, 0.60, 0.70,
        0.80, 0.85, 0.90, 0.95, 1.00,
    ];

    lut_data.get_array_mut().resize(DIM as _, 1).unwrap();
    for (i, &entry) in lut_entries.iter().enumerate() {
        lut_data.get_array_mut()[i * 3] = entry;
        lut_data.get_array_mut()[i * 3 + 1] = entry;
        lut_data.get_array_mut()[i * 3 + 2] = entry;
    }

    let mut inv_lut = lut_data.inverse();

    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();

    let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::F16, BitDepth::F16).unwrap();

    let in_image: Vec<half::f16> = [
        1.00f32, 0.91, 0.85, 0.0, //
        0.75, 0.02, 0.53, 0.0, //
        0.47, 0.30, 0.21, 0.0, //
        0.50, 0.11, 0.00, 0.0,
    ]
    .into_iter()
    .map(half)
    .collect();

    let mut out_image = vec![half(-1.0); 16];

    // (dist + (val-low)/(high-low)) / (dim-1)
    let expected: Vec<half::f16> = [
        1.0000000000f32,
        0.8714285714,
        0.7857142857,
        0.0, //
        0.6785714285,
        0.0285714285,
        0.5214285714,
        0.0, //
        0.4785714285,
        0.3571428571,
        0.2928571428,
        0.0, //
        0.5000000000,
        0.1571428571,
        0.0000000000,
        0.0,
    ]
    .into_iter()
    .map(half)
    .collect();

    cpu_op.apply_bit_depth(Pixels::F16(&in_image), PixelsMut::F16(&mut out_image));

    for i in 0..4 * 4 {
        check_close(
            f32::from(out_image[i].to_bits()),
            f32::from(expected[i].to_bits()),
            1.1f32,
        );
    }

    // Repeat with FAST.
    let const_inv_lut = fast_from_inverse(&mut inv_lut);
    let cpu_op_fast = get_lut1d_renderer(&const_inv_lut, BitDepth::F16, BitDepth::F16).unwrap();

    cpu_op_fast.apply_bit_depth(Pixels::F16(&in_image), PixelsMut::F16(&mut out_image));

    for i in 0..4 * 4 {
        check_close(
            f32::from(out_image[i].to_bits()),
            f32::from(expected[i].to_bits()),
            1.1f32,
        );
    }
}

/// The fast LUT's renderer from F32 input runs once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DRenderer, lut_1d_inv_half_identity)` @ v2.5.2.
#[test]
fn lut_1d_inv_half_identity() {
    // Need to do 10i-->32f and vice versa to check that
    // both the in scaling and out scaling are working correctly.

    const STEPUI: u16 = 700; // relative to 10i
    let step = STEPUI as f32 / 1023.0f32;

    // Process from 10i to 32f bit-depths.
    {
        // By default, this constructor creates an 'identity LUT'.
        let mut lut_data =
            Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
        lut_data.set_file_output_bit_depth(BitDepth::Uint10);

        let mut inv_lut = lut_data.inverse();

        inv_lut.validate().unwrap();
        inv_lut.finalize().unwrap();

        let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::Uint10, BitDepth::F32).unwrap();

        #[rustfmt::skip]
        let in_image: [u16; 20] = [
            0,      0,      0,      0,
            STEPUI, 0,      0,      0,
            0,      STEPUI, 0,      0,
            0,      0,      STEPUI, 0,
            STEPUI, STEPUI, STEPUI, 0,
        ];

        let mut out_image = [-1.0f32; 20];

        // Inverse of identity should still be identity.
        #[rustfmt::skip]
        let expected: [f32; 20] = [
            0.0,  0.0,  0.0,  0.0,
            step, 0.0,  0.0,  0.0,
            0.0,  step, 0.0,  0.0,
            0.0,  0.0,  step, 0.0,
            step, step, step, 0.0,
        ];

        cpu_op.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::F32(&mut out_image));

        for i in 0..5 * 4 {
            check_close(out_image[i], expected[i], 1e-6f32);
        }

        // Repeat with FAST.
        let const_inv_lut = fast_from_inverse(&mut inv_lut);
        let cpu_op_fast =
            get_lut1d_renderer(&const_inv_lut, BitDepth::Uint10, BitDepth::F32).unwrap();

        cpu_op_fast.apply_bit_depth(Pixels::U16(&in_image), PixelsMut::F32(&mut out_image));

        for i in 0..5 * 4 {
            check_close(out_image[i], expected[i], 1e-6f32);
        }
    }
    // Process from 32f to 10i bit-depths.
    {
        // By default, this constructor creates an 'identity LUT'.
        let mut lut_data =
            Lut1DOpData::with_half_flags(HalfFlags::INPUT_HALF_CODE, 65536, false).unwrap();
        lut_data.set_file_output_bit_depth(BitDepth::F32);

        let mut inv_lut = lut_data.inverse();

        inv_lut.validate().unwrap();
        inv_lut.finalize().unwrap();

        let cpu_op = get_lut1d_renderer(&inv_lut, BitDepth::F32, BitDepth::Uint10).unwrap();

        #[rustfmt::skip]
        let in_image: [f32; 20] = [
            0.0,  0.0,  0.0,  0.0,
            step, 0.0,  0.0,  0.0,
            0.0,  step, 0.0,  0.0,
            0.0,  0.0,  step, 0.0,
            step, step, step, 0.0,
        ];

        let mut out_image = [10000u16; 20];

        // Inverse of identity should still be identity.
        #[rustfmt::skip]
        let expected: [u16; 20] = [
            0,      0,      0,      0,
            STEPUI, 0,      0,      0,
            0,      STEPUI, 0,      0,
            0,      0,      STEPUI, 0,
            STEPUI, STEPUI, STEPUI, 0,
        ];
        cpu_op.apply_bit_depth(Pixels::F32(&in_image), PixelsMut::U16(&mut out_image));

        for i in 0..5 * 4 {
            assert_eq!(out_image[i], expected[i]);
        }

        // Repeat with FAST.
        let const_inv_lut = fast_from_inverse(&mut inv_lut);
        for cpu in simd_modes() {
            let cpu_op_fast =
                get_lut1d_renderer_for_cpu(&const_inv_lut, BitDepth::F32, BitDepth::Uint10, &cpu)
                    .unwrap();

            cpu_op_fast.apply_bit_depth(Pixels::F32(&in_image), PixelsMut::U16(&mut out_image));

            for i in 0..5 * 4 {
                assert_eq!(out_image[i], expected[i]);
            }
        }
    }
}
