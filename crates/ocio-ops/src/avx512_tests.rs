// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/AVX512_tests.cpp, registered as in tests/cpu/SIMD_tests.cpp:79-100 @ v2.5.2.
//! The bodies are those of `SSE2_tests.cpp` ([`crate::sse2::tests`]), over the AVX-512 pack,
//! with the `LoadMasked`/`StoreMasked` checks of AVX512_tests.cpp:121-155 and the 64-value
//! `check_packed_nan_inf` (the 32 values listed twice).

use crate::avx512::{
    avx512_rgba_pack_load, avx512_rgba_pack_load_masked, avx512_rgba_pack_store,
    avx512_rgba_pack_store_masked_writes,
};
use crate::sse2::PackDepth;
use crate::sse2::tests::{
    Masked, Pack, check_packed_all, check_packed_f16_to_f32, check_packed_nan_inf,
    check_packed_uint_to_f32,
};

const AVX512: Pack = Pack {
    load: avx512_rgba_pack_load,
    store: avx512_rgba_pack_store,
    masked: Some(Masked {
        load: avx512_rgba_pack_load_masked,
        writes: avx512_rgba_pack_store_masked_writes,
        values: 64,
    }),
};

/// Port of `OCIO_ADD_TEST(AVX512, packed_uint8_to_float_test)` @ v2.5.2.
#[test]
fn packed_uint8_to_float_test() {
    check_packed_uint_to_f32(AVX512, PackDepth::Uint8);
}

/// Port of `OCIO_ADD_TEST(AVX512, packed_uint10_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint10_to_f32_test() {
    check_packed_uint_to_f32(AVX512, PackDepth::Uint10);
}

/// Port of `OCIO_ADD_TEST(AVX512, packed_uint12_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint12_to_f32_test() {
    check_packed_uint_to_f32(AVX512, PackDepth::Uint12);
}

/// Port of `OCIO_ADD_TEST(AVX512, packed_uint16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint16_to_f32_test() {
    check_packed_uint_to_f32(AVX512, PackDepth::Uint16);
}

/// Port of `OCIO_ADD_TEST(AVX512, packed_f16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_f16_to_f32_test() {
    check_packed_f16_to_f32(AVX512);
}

/// Port of `OCIO_ADD_TEST(AVX512, packed_nan_inf_test)` @ v2.5.2.
#[test]
fn packed_nan_inf_test() {
    check_packed_nan_inf(AVX512, 2, true);
}

/// Port of `OCIO_ADD_TEST(AVX512, packed_all_test)` @ v2.5.2.
#[test]
fn packed_all_test() {
    check_packed_all(AVX512, true);
}
