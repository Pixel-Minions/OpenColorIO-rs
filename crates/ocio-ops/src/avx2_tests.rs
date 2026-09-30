// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/AVX2_tests.cpp, registered as in tests/cpu/SIMD_tests.cpp:54-77 @ v2.5.2.
//! The bodies are those of `SSE2_tests.cpp` ([`crate::sse2::tests`]), over the AVX2 pack.

use crate::avx2::{avx2_rgba_pack_load, avx2_rgba_pack_store};
use crate::sse2::PackDepth;
use crate::sse2::tests::{
    Pack, check_packed_all, check_packed_f16_to_f32, check_packed_nan_inf, check_packed_uint_to_f32,
};

const AVX2: Pack = Pack {
    load: avx2_rgba_pack_load,
    store: avx2_rgba_pack_store,
    masked: None,
};

/// Port of `OCIO_ADD_TEST(AVX2, packed_uint8_to_float_test)` @ v2.5.2.
#[test]
fn packed_uint8_to_float_test() {
    check_packed_uint_to_f32(AVX2, PackDepth::Uint8);
}

/// Port of `OCIO_ADD_TEST(AVX2, packed_uint10_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint10_to_f32_test() {
    check_packed_uint_to_f32(AVX2, PackDepth::Uint10);
}

/// Port of `OCIO_ADD_TEST(AVX2, packed_uint12_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint12_to_f32_test() {
    check_packed_uint_to_f32(AVX2, PackDepth::Uint12);
}

/// Port of `OCIO_ADD_TEST(AVX2, packed_uint16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_uint16_to_f32_test() {
    check_packed_uint_to_f32(AVX2, PackDepth::Uint16);
}

/// Port of `OCIO_ADD_TEST(AVX2, packed_f16_to_f32_test)` @ v2.5.2.
#[test]
fn packed_f16_to_f32_test() {
    check_packed_f16_to_f32(AVX2);
}

/// Port of `OCIO_ADD_TEST(AVX2, packed_nan_inf_test)` @ v2.5.2.
#[test]
fn packed_nan_inf_test() {
    check_packed_nan_inf(AVX2, 1, true);
}

/// Port of `OCIO_ADD_TEST(AVX2, packed_all_test)` @ v2.5.2.
#[test]
fn packed_all_test() {
    check_packed_all(AVX2, true);
}
