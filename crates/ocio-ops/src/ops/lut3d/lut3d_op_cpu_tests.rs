// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/lut3d/Lut3DOpCPU_tests.cpp @ v2.5.2.

use ocio_testkit::upstream::{check_close, check_equal};

use super::*;
use crate::cpu_info::{
    X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512, X86_CPU_FLAG_SSE2,
};

/// The CPU flags upstream's test runner forces for each SIMD mode (`--no_accel`, `-sse2`,
/// `-avx`, `-avx2`, `-avx512`; tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2). The runner reruns
/// the whole test binary per mode; here each test body runs once per mode.
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

/// Port of `Lut3DRendererNaNTest` (tests/cpu/ops/lut3d/Lut3DOpCPU_tests.cpp:14-46 @ v2.5.2).
fn lut3d_renderer_nan_test(interpol: Interpolation, cpu: &CpuInfo) {
    let mut lut = Lut3DOpData::new(interpol, 4).expect("a 4^3 LUT");

    // Change LUT so that it is not identity.
    lut.array_mut().values_mut()[65] += 0.001f32;
    let values = lut.array().values().to_vec();

    // GetLut3DRenderer of a forward LUT is GetForwardLut3DRenderer.
    let renderer = get_forward_lut3d_renderer(&lut, cpu);

    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let mut pixels: [f32; 16] = [ qnan, qnan, qnan, 0.5,
                                  0.5,  0.3,  0.2,  qnan,
                                  inf,  inf,  inf,  inf,
                                 -inf, -inf, -inf, -inf ];

    // renderer->apply(pixels, pixels, 4)
    renderer.apply(&mut pixels);

    check_close(pixels[0], values[0], 1e-7f32);
    check_close(pixels[1], values[1], 1e-7f32);
    check_close(pixels[2], values[2], 1e-7f32);
    assert!(pixels[7].is_nan(), "FAILED: OCIO::IsNan(pixels[7])");
    check_close(pixels[8], 1.0f32, 1e-7f32);
    check_close(pixels[9], 1.0f32, 1e-7f32);
    check_close(pixels[10], 1.0f32, 1e-7f32);
    check_equal(pixels[11], inf);
    check_close(pixels[12], 0.0f32, 1e-7f32);
    check_close(pixels[13], 0.0f32, 1e-7f32);
    check_close(pixels[14], 0.0f32, 1e-7f32);
    check_equal(pixels[15], -inf);
}

/// Port of `OCIO_ADD_TEST(Lut3DRenderer, nan_linear_test)` @ v2.5.2.
#[test]
fn nan_linear_test() {
    for cpu in simd_modes() {
        lut3d_renderer_nan_test(Interpolation::Linear, &cpu);
    }
}

/// Port of `OCIO_ADD_TEST(Lut3DRenderer, nan_tetra_test)` @ v2.5.2.
#[test]
fn nan_tetra_test() {
    for cpu in simd_modes() {
        lut3d_renderer_nan_test(Interpolation::Tetrahedral, &cpu);
    }
}
