// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/AVX_tests.cpp, registered as in tests/cpu/SIMD_tests.cpp:29-52 @ v2.5.2.
//! The bodies are those of `SSE2_tests.cpp` ([`crate::sse2::tests`]), over the AVX pack.

use crate::avx::{avx_rgba_pack_load, avx_rgba_pack_store};
use crate::sse2::PackDepth;
use crate::sse2::tests::{
    Pack, check_packed_all, check_packed_f16_to_f32, check_packed_nan_inf, check_packed_uint_to_f32,
};

const AVX: Pack = Pack {
    load: avx_rgba_pack_load,
    store: avx_rgba_pack_store,
    masked: None,
};

/// Port of `OCIO_ADD_TEST(AVX, packed_uint8_to_float_test)` @ v2.5.2.
#[test]
fn packed_uint8_to_float_test() {
    check_packed_uint_to_f32(AVX, PackDepth::Uint8);
}

/// Port of `OCIO_ADD_TEST(AVX, packed_uint10_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint10_to_f32_test() {
    check_packed_uint_to_f32(AVX, PackDepth::Uint10);
}

/// Port of `OCIO_ADD_TEST(AVX, packed_uint12_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint12_to_f32_test() {
    check_packed_uint_to_f32(AVX, PackDepth::Uint12);
}

/// Port of `OCIO_ADD_TEST(AVX, packed_uint16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint16_to_f32_test() {
    check_packed_uint_to_f32(AVX, PackDepth::Uint16);
}

/// Port of `OCIO_ADD_TEST(AVX, packed_f16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_f16_to_f32_test() {
    check_packed_f16_to_f32(AVX);
}

/// Port of `OCIO_ADD_TEST(AVX, packed_nan_inf_test)` @ v2.5.2.
#[test]
fn packed_nan_inf_test() {
    check_packed_nan_inf(AVX, 1, true);
}

/// Port of `OCIO_ADD_TEST(AVX, packed_all_test)` @ v2.5.2.
#[test]
fn packed_all_test() {
    check_packed_all(AVX, true);
}
