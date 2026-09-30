// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/SSE2_tests.cpp, registered as in tests/cpu/SIMD_tests.cpp:6-27 @ v2.5.2.
//!
//! `SSE2_tests.cpp`, `AVX_tests.cpp`, `AVX2_tests.cpp` and `AVX512_tests.cpp` are the same tests
//! over each SIMD pack, with 16, 32 or 64 values per `Load`/`Store`. The bodies are ported once
//! here, over a [`Pack`]; the AVX files call them with their pack. The packs' values never mix,
//! so the port applies `Load` and `Store` value by value (see [`crate::sse2::rgba_pack_load`]).
//!
//! Upstream skips a SIMD mode the CPU lacks (`SSE2_CHECK`, `HAS_F16C`); the port's packs are
//! scalar code, so every test runs on every machine.

use ocio_testkit::upstream::math_utils::floats_differ;

use crate::imath_half;
use crate::sse2::{PackDepth, sse2_rgba_pack_load, sse2_rgba_pack_store};

/// An RGBA pack under test: `XRGBAPack<BD>::Load` and `::Store`, one value at a time.
#[derive(Clone, Copy)]
pub(crate) struct Pack {
    pub(crate) load: fn(PackDepth, u32) -> f32,
    pub(crate) store: fn(PackDepth, f32) -> u32,
    /// Whether the test runs `LoadMasked`/`StoreMasked` too (AVX-512).
    pub(crate) masked: Option<Masked>,
}

/// The masked `Load`/`Store` of a pack (AVX-512): value `index` of a block, `pixel_count` pixels.
#[derive(Clone, Copy)]
pub(crate) struct Masked {
    pub(crate) load: fn(PackDepth, u32, usize, usize) -> f32,
    pub(crate) writes: fn(usize, usize) -> bool,
    /// Values per masked call (64: 16 RGBA pixels).
    pub(crate) values: usize,
}

const SSE2: Pack = Pack {
    load: sse2_rgba_pack_load,
    store: sse2_rgba_pack_store,
    masked: None,
};

/// A stored value as C++ `(float)value`: integers exactly, halves through Imath's
/// `half::operator float`, floats unchanged.
fn to_float(depth: PackDepth, raw: u32) -> f32 {
    match depth {
        PackDepth::F16 => imath_half::half_to_float(raw as u16),
        PackDepth::F32 => f32::from_bits(raw),
        _ => raw as f32,
    }
}

/// Port of `scale_unsigned<BD>` (tests/cpu/SSE2_tests.cpp:58-74 @ v2.5.2): the test input value
/// for index `i`, as stored.
fn scale_unsigned(depth: PackDepth, i: u32) -> u32 {
    match depth {
        // static_cast<float>(i) * 1.0f/65535.0f
        PackDepth::F32 => (i as f32 * 1.0f32 / 65535.0f32).to_bits(),
        // static_cast<half>(1.0f/65535.0f * static_cast<float>(i))
        PackDepth::F16 => u32::from(imath_half::float_to_half(1.0f32 / 65535.0f32 * i as f32)),
        _ => i,
    }
}

/// Port of `testConvert_OutBitDepth<inBD, outBD>` (tests/cpu/SSE2_tests.cpp:76-120 @ v2.5.2),
/// with the masked part of tests/cpu/AVX512_tests.cpp:121-155 for AVX-512.
fn test_convert_out_bit_depth(pack: Pack, in_bd: PackDepth, out_bd: PackDepth) {
    let max_value = if in_bd.is_float() {
        65536
    } else {
        in_bd.max_value() as usize + 1
    };

    let in_image: Vec<u32> = (0..max_value as u32)
        .map(|i| scale_unsigned(in_bd, i))
        .collect();

    let scale = out_bd.max_value() as f32 / in_bd.max_value() as f32;

    let out_image: Vec<u32> = in_image
        .iter()
        .map(|&raw| (pack.store)(out_bd, (pack.load)(in_bd, raw) * scale))
        .collect();

    // The expected value: (float)in * scale, cast to half for F16, rintf for integers.
    let expected = |v: f32| {
        if out_bd.is_float() {
            if out_bd == PackDepth::F16 {
                imath_half::half_to_float(imath_half::float_to_half(v))
            } else {
                v
            }
        } else {
            v.round_ties_even()
        }
    };

    for (i, (&raw_in, &raw_out)) in in_image.iter().zip(&out_image).enumerate() {
        let v = expected(to_float(in_bd, raw_in) * scale);
        let actual = to_float(out_bd, raw_out);
        assert!(
            !floats_differ(v, actual, 0, false),
            "expected: {v} != actual: {actual} : {in_bd:?} -> {out_bd:?} (value {i})"
        );
    }

    // Test Load/Store Masked.
    let Some(masked) = pack.masked else {
        return;
    };
    for pixel_count in 0..=16 {
        // reset all values to zero
        let mut out_image = vec![0u32; in_image.len()];

        for (j, out) in out_image.iter_mut().enumerate().take(masked.values) {
            let loaded = (masked.load)(in_bd, in_image[j], j, pixel_count) * scale;
            if (masked.writes)(j, pixel_count) {
                *out = (pack.store)(out_bd, loaded);
            }
        }

        for (i, (&raw_in, &raw_out)) in in_image.iter().zip(&out_image).enumerate() {
            let mut v = to_float(in_bd, raw_in) * scale;

            // values geater then the pixel count should not have been written to
            if i >= pixel_count * 4 {
                v = 0.0;
            }
            let v = expected(v);
            let actual = to_float(out_bd, raw_out);
            assert!(
                !floats_differ(v, actual, 0, false),
                "expected: {v} != actual: {actual} : {in_bd:?} -> {out_bd:?} (value {i}, {pixel_count} pixels)"
            );
        }
    }
}

/// Port of `packed_uint8_to_float_test` and its UINT10, UINT12 and UINT16 siblings
/// (tests/cpu/SSE2_tests.cpp:151-256 @ v2.5.2): every integer, loaded and stored as F32.
pub(crate) fn check_packed_uint_to_f32(pack: Pack, in_bd: PackDepth) {
    let max_value = in_bd.max_value() + 1;
    for i in 0..max_value {
        let out = f32::from_bits((pack.store)(PackDepth::F32, (pack.load)(in_bd, i)));
        assert!(
            !floats_differ(i as f32, out, 0, false),
            "expected: {} != actual: {out} : {in_bd:?} -> F32",
            i as f32
        );
    }
}

/// Port of `packed_f16_to_f32_test` (tests/cpu/SSE2_tests.cpp:258-283 @ v2.5.2): every half,
/// loaded and stored as F32, against Imath's conversion.
pub(crate) fn check_packed_f16_to_f32(pack: Pack) {
    for bits in 0..=u32::from(u16::MAX) {
        let out = f32::from_bits((pack.store)(
            PackDepth::F32,
            (pack.load)(PackDepth::F16, bits),
        ));
        let expected = imath_half::half_to_float(bits as u16);
        assert!(
            !floats_differ(expected, out, 0, false),
            "expected: {expected} != actual: {out} : F16 -> F32 (half {bits:#06x})"
        );
    }
}

/// The input pixels of `check_packed_nan_inf` (tests/cpu/SSE2_tests.cpp:297-304 @ v2.5.2).
/// `AVX512_tests.cpp` lists these 32 values twice, `AVX_tests.cpp` and `AVX2_tests.cpp` once.
pub(crate) fn nan_inf_pixels() -> [f32; 32] {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    let maxf = f32::MAX;
    #[rustfmt::skip]
    let pixels = [     qnan,      qnan,       qnan,     0.25f32,
                       maxf,     -maxf,       3.2f32,    qnan,
                        inf,       inf,        inf,       inf,
                       -inf,      -inf,       -inf,      -inf,
                       0.0f32,  270.0f32,   500.0f32,    2.0f32,
                      -0.0f32,   -1.0f32,   - 2.0f32,   -5.0f32,
                  100000.0f32, 200000.0f32, -10.0f32, -2000.0f32,
                   65535.0f32, 65537.0f32, -65536.0f32, -65537.0f32 ];
    pixels
}

/// `resultU8` of `check_packed_nan_inf` (tests/cpu/SSE2_tests.cpp:320-327 @ v2.5.2).
#[rustfmt::skip]
const RESULT_U8: [u32; 32] = [   0,   0,   0,   0,
                               255,   0,   3,   0,
                               255, 255, 255, 255,
                                 0,   0,   0,   0,
                                 0, 255, 255,   2,
                                 0,   0,   0,   0,
                               255, 255,   0,   0,
                               255, 255,   0,   0 ];

/// `resultU10` (tests/cpu/SSE2_tests.cpp:341-348 @ v2.5.2).
#[rustfmt::skip]
const RESULT_U10: [u32; 32] = [    0,    0,    0,    0,
                                1023,    0,    3,    0,
                                1023, 1023, 1023, 1023,
                                   0,    0,    0,    0,
                                   0,  270,  500,    2,
                                   0,    0,    0,    0,
                                1023, 1023,    0,    0,
                                1023, 1023,    0,    0];

/// `resultU12` (tests/cpu/SSE2_tests.cpp:363-370 @ v2.5.2).
#[rustfmt::skip]
const RESULT_U12: [u32; 32] = [    0,    0,    0,    0,
                                4095,    0,    3,    0,
                                4095, 4095, 4095, 4095,
                                   0,    0,    0,    0,
                                   0,  270,  500,    2,
                                   0,    0,    0,    0,
                                4095, 4095,    0,    0,
                                4095, 4095,    0,    0];

/// `resultU16` (tests/cpu/SSE2_tests.cpp:385-392 @ v2.5.2).
#[rustfmt::skip]
const RESULT_U16: [u32; 32] = [    0,     0,     0,     0,
                               65535,     0,     3,     0,
                               65535, 65535, 65535, 65535,
                                   0,     0,     0,     0,
                                   0,   270,   500,     2,
                                   0,     0,     0,     0,
                               65535, 65535,     0,     0,
                               65535, 65535,     0,     0];

/// Port of `packed_nan_inf_test` (tests/cpu/SSE2_tests.cpp:286-407 @ v2.5.2): the specials,
/// stored as F16 and as each integer depth. `repeats` is how many times the file lists the 32
/// values (2 in `AVX512_tests.cpp`); `f16` is false when upstream skips the F16 part.
pub(crate) fn check_packed_nan_inf(pack: Pack, repeats: usize, f16: bool) {
    let pixels: Vec<f32> = nan_inf_pixels().repeat(repeats);

    if f16 {
        for (i, &p) in pixels.iter().enumerate() {
            let raw = (pack.store)(PackDepth::F16, (pack.load)(PackDepth::F32, p.to_bits()));
            // (half)pixels[i], compared as a float.
            let expected = imath_half::half_to_float(imath_half::float_to_half(p));
            let actual = imath_half::half_to_float(raw as u16);
            assert!(
                !floats_differ(expected, actual, 0, false),
                "expected: {expected} != actual: {actual} : F32 -> F16 (value {i})"
            );
        }
    }

    for (depth, results) in [
        (PackDepth::Uint8, RESULT_U8),
        (PackDepth::Uint10, RESULT_U10),
        (PackDepth::Uint12, RESULT_U12),
        (PackDepth::Uint16, RESULT_U16),
    ] {
        let results = results.repeat(repeats);
        for (i, (&p, &expected)) in pixels.iter().zip(&results).enumerate() {
            let actual = (pack.store)(depth, (pack.load)(PackDepth::F32, p.to_bits()));
            assert!(
                !floats_differ(expected as f32, actual as f32, 0, false),
                "expected: {expected} != actual: {actual} : F32 -> {depth:?} (value {i})"
            );
        }
    }
}

/// Port of `packed_all_test` (tests/cpu/SSE2_tests.cpp:409-455 @ v2.5.2): every pair of bit
/// depths through `testConvert_InBitDepth`. `f16` is false when upstream skips F16 in and out.
pub(crate) fn check_packed_all(pack: Pack, f16: bool) {
    let formats = [
        PackDepth::Uint8,
        PackDepth::Uint10,
        PackDepth::Uint12,
        PackDepth::Uint16,
        PackDepth::F16,
        PackDepth::F32,
    ];
    for in_bd in formats {
        for out_bd in formats {
            if !f16 && (in_bd == PackDepth::F16 || out_bd == PackDepth::F16) {
                continue;
            }
            test_convert_out_bit_depth(pack, in_bd, out_bd);
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_uint8_to_float_test)` @ v2.5.2.
#[test]
fn packed_uint8_to_float_test() {
    check_packed_uint_to_f32(SSE2, PackDepth::Uint8);
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_uint10_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint10_to_f32_test() {
    check_packed_uint_to_f32(SSE2, PackDepth::Uint10);
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_uint12_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint12_to_f32_test() {
    check_packed_uint_to_f32(SSE2, PackDepth::Uint12);
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_uint16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint16_to_f32_test() {
    check_packed_uint_to_f32(SSE2, PackDepth::Uint16);
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_f16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_f16_to_f32_test() {
    check_packed_f16_to_f32(SSE2);
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_nan_inf_test)` @ v2.5.2.
#[test]
fn packed_nan_inf_test() {
    check_packed_nan_inf(SSE2, 1, true);
}

/// Port of `OCIO_ADD_TEST(SSE2, packed_all_test)` @ v2.5.2.
#[test]
fn packed_all_test() {
    check_packed_all(SSE2, true);
}
