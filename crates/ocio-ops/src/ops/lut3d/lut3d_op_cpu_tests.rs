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
    let mut lut = Lut3DOpData::with_interpolation(interpol, 4).expect("a 4^3 LUT");

    // Change LUT so that it is not identity.
    lut.get_array_mut().get_values_mut()[65] += 0.001f32;
    let values = lut.get_array().get_values().clone();

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

/// The real kernels this CPU can run give the bits of their exact lanes (PLAN.md, "equality tests
/// against the scalar profile"), on rows with and without specials, of every length around
/// their groups, on a smooth table and on one whose neighbours overflow when added. The lanes
/// are compared with the wheel in `tests/lut3d_oracle.rs`, on the CPU each one is dispatched
/// on.
#[cfg(target_arch = "x86_64")]
#[test]
fn real_kernels_match_their_lanes() {
    use crate::ops::lut3d::{
        lut3d_op_cpu_avx as avx, lut3d_op_cpu_avx2 as avx2, lut3d_op_cpu_avx512 as avx512,
        lut3d_op_cpu_sse2 as sse2,
    };
    use ocio_testkit::probe;

    type Lanes = fn(&[f32], i32, &mut [f32], i32);
    type Real = fn(&[f32], i32, &mut [f32]) -> usize;

    let kernels: [(&str, Real, Lanes, bool); 4] = [
        (
            "SSE2",
            sse2::simd::tetrahedral,
            sse2::apply_tetrahedral_lanes,
            true,
        ),
        (
            "AVX",
            avx::simd::tetrahedral,
            avx::apply_tetrahedral_lanes,
            std::arch::is_x86_feature_detected!("avx"),
        ),
        (
            "AVX2",
            avx2::simd::tetrahedral,
            avx2::apply_tetrahedral_lanes,
            std::arch::is_x86_feature_detected!("avx2")
                && std::arch::is_x86_feature_detected!("fma"),
        ),
        (
            "AVX-512",
            avx512::simd::tetrahedral,
            avx512::apply_tetrahedral_lanes,
            std::arch::is_x86_feature_detected!("avx512f"),
        ),
    ];

    let dim = 5i32;
    let entries = (dim * dim * dim) as usize;
    let smooth: Vec<f32> = (0..entries)
        .flat_map(|i| {
            let x = i as f32 / entries as f32;
            [x.sqrt(), x * 1.5 - 0.25, 1.0 - x, 0.0]
        })
        .collect();
    let extreme: Vec<f32> = (0..entries)
        .flat_map(|i| {
            let v = if i % 3 == 0 { f32::MAX } else { f32::MIN };
            [v, -v, v, 0.0]
        })
        .collect();

    let mut rows = probe::finite_rows();
    rows.extend(probe::row_length_buffers(33));
    rows.extend(probe::nan_buffers(24));

    let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect::<Vec<u32>>();
    let mut checked = 0;
    for (name, real, lanes, runs) in kernels {
        if !runs {
            continue;
        }
        for table in [&smooth, &extreme] {
            for (row_name, row) in &rows {
                let mut a = row.clone();
                let mut b = row.clone();
                let done = real(table, dim, &mut a);
                assert!(row.len() - done < 64, "{name}: {row_name}");
                lanes(table, dim, &mut a[done..], ((row.len() - done) / 4) as i32);
                lanes(table, dim, &mut b, (row.len() / 4) as i32);
                assert_eq!(bits(&a), bits(&b), "{name}: {row_name}");
                checked += usize::from(done > 0);
            }
        }
    }
    assert!(checked > rows.len(), "{checked}");

    // Trilinear: the real SSE2 loop against its lanes, through the renderer's own table.
    let cpu = CpuInfo::instance().with_flags(0);
    for values in [0.75f32, f32::MAX] {
        let mut data = Lut3DOpData::new(dim as _).unwrap();
        let array = data.get_array_mut().get_values_mut();
        for (i, value) in array.iter_mut().enumerate() {
            *value = if i % 3 == 1 {
                -values
            } else {
                values * (i % 7) as f32 / 6.0
            };
        }
        let renderer = Lut3DRenderer::new(&data, &cpu);
        for (row_name, row) in &rows {
            let mut a = row.clone();
            let mut b = row.clone();
            renderer.apply_sse2(&mut a);
            renderer.apply_sse2_lanes(&mut b);
            assert_eq!(bits(&a), bits(&b), "trilinear: {row_name}");
        }
    }
}
