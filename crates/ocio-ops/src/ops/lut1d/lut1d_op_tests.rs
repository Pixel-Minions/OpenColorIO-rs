// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOp_tests.cpp` @ v2.5.2: the tests that need no
//! composition or inverse (WP 2.1e to 2.1g), and tests of the refusals that stand in for them
//! until then.
//!
//! Upstream's test runner reruns every test in each SIMD mode the CPU supports
//! (tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2); a test of a renderer with SIMD kernels runs
//! once per mode here, through `GetLut1DRenderer` with that mode's CPU flags (what
//! `Op::apply`'s `getCPUOp(false)` builds).

use super::*;
use crate::cpu_info::{
    CpuInfo, X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512, X86_CPU_FLAG_SSE2,
};
use crate::open_color_types::{BitDepth, OptimizationFlags};
use crate::ops::lut1d::lut1d_op_cpu::get_lut1d_renderer_for_cpu;
use ocio_testkit::upstream::check_close;

/// `CreateSquareLut` (tests/cpu/ops/lut1d/Lut1DOp_tests.cpp:126-144 @ v2.5.2): a LUT that
/// squares the input.
fn create_square_lut() -> Lut1DOpData {
    const SIZE: i32 = 256;
    let mut lut = Lut1DOpData::new(SIZE as _).unwrap();
    let lut_array = lut.get_array_mut();

    for i in 0..SIZE {
        let x = i as f32 / (SIZE - 1) as f32;
        let x2 = x * x;

        for c in 0..3 {
            lut_array[(c + i * 3) as usize] = x2;
        }
    }
    lut
}

/// The CPU flags of upstream's SIMD modes (`lut1d_op_cpu_tests.rs`).
fn simd_modes() -> Vec<CpuInfo> {
    let cpu = CpuInfo::instance();
    let mut modes = vec![cpu.with_flags(0)];
    for flag in [
        X86_CPU_FLAG_SSE2,
        X86_CPU_FLAG_AVX,
        X86_CPU_FLAG_AVX2,
        X86_CPU_FLAG_AVX512,
    ] {
        if cpu.flags & flag != 0 {
            modes.push(cpu.with_flags(flag));
        }
    }
    modes
}

/// Once per SIMD mode (module docs).
///
/// Port of `OCIO_ADD_TEST(Lut1DOp, extrapolation_errors)` @ v2.5.2.
#[test]
fn extrapolation_errors() {
    let mut lut = Lut1DOpData::new(3).unwrap();
    let lut_array = lut.get_array_mut();

    // Simple y=x+0.1 LUT.
    for i in 0..3 {
        for c in 0..3 {
            lut_array[c + i * 3] += 0.1f32;
        }
    }

    let is_identity = lut.is_no_op();
    assert!(!is_identity);

    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut, TransformDirection::Forward);

    const PIXELS: usize = 5;
    #[rustfmt::skip]
    let input_buffer_linearforward: [f32; PIXELS * 4] = [
        -0.1, -0.2, -10.0, 0.0,
        0.5, 1.0, 1.1, 0.0,
        10.1, 55.0, 2.3, 0.0,
        9.1, 1.0e6, 1.0e9, 0.0,
        4.0e9, 9.5e7, 0.5, 0.0,
    ];
    #[rustfmt::skip]
    let output_buffer_linearforward: [f32; PIXELS * 4] = [
        0.1, 0.1, 0.1, 0.0,
        0.6, 1.1, 1.1, 0.0,
        1.1, 1.1, 1.1, 0.0,
        1.1, 1.1, 1.1, 0.0,
        1.1, 1.1, 0.6, 0.0,
    ];

    let OpData::Lut1D(data) = &**ops[0].data() else {
        unreachable!("a Lut1D op")
    };
    for cpu in simd_modes() {
        let mut buffer = input_buffer_linearforward;
        get_lut1d_renderer_for_cpu(data, BitDepth::F32, BitDepth::F32, &cpu)
            .unwrap()
            .apply(&mut buffer);
        for i in 0..buffer.len() {
            check_close(buffer[i], output_buffer_linearforward[i], 1e-5f32);
        }
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOp, gpu)` @ v2.5.2.
#[test]
fn gpu() {
    let lut = create_square_lut();
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut, TransformDirection::Forward);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);
    assert!(!ops[0].supported_by_legacy_shader());
}

/// Port of `OCIO_ADD_TEST(Lut1DOp, identity_lut_1d)` @ v2.5.2.
#[test]
fn identity_lut_1d() {
    let mut size = 3;
    let mut channels = 2;
    let mut data = vec![0.0f32; (size * channels) as usize];
    generate_identity_lut1d(&mut data, size, channels);
    assert_eq!(data[0], 0.0f32);
    assert_eq!(data[1], 0.0f32);
    assert_eq!(data[2], 0.5f32);
    assert_eq!(data[3], 0.5f32);
    assert_eq!(data[4], 1.0f32);
    assert_eq!(data[5], 1.0f32);

    size = 4;
    channels = 3;
    data.resize((size * channels) as usize, 0.0);
    generate_identity_lut1d(&mut data, size, channels);
    for c in 0..channels as usize {
        let channels = channels as usize;
        assert_eq!(data[c], 0.0f32);
        assert_eq!(data[channels + c], 0.33333333f32);
        assert_eq!(data[2 * channels + c], 0.66666667f32);
        assert_eq!(data[3 * channels + c], 1.0f32);
    }
}

/// What waits for the rest of Phase 2 is an error: composing two LUTs, and the inverse LUT's
/// renderers. The float renderers (`getCPUOp`, `apply`) and the inverse's set-up (`finalize`)
/// exist.
#[test]
fn phase_2_parts_are_errors() {
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, create_square_lut(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, create_square_lut(), TransformDirection::Forward);
    ops.finalize().unwrap();

    ops[0].get_cpu_op(false).unwrap().expect("a renderer");
    ops[0].get_cpu_op(true).unwrap().expect("a renderer");
    let mut pixel = [0.5f32, 0.25, 0.125, 1.0];
    ops[0].apply(&mut pixel).unwrap();

    // Two LUTs that may compose.
    assert!(ops[0].can_combine_with(&ops[1]).unwrap());
    let mut out = OpVec::new();
    assert_eq!(
        ops[0]
            .combine_with(&mut out, &ops[1])
            .unwrap_err()
            .message(),
        NOT_PORTED_COMPOSE
    );
    assert_eq!(
        ops.optimize(OptimizationFlags::DEFAULT)
            .unwrap_err()
            .message(),
        NOT_PORTED_COMPOSE
    );

    // An inverse LUT.
    let mut inverse = OpVec::new();
    create_lut1d_op(
        &mut inverse,
        create_square_lut(),
        TransformDirection::Inverse,
    );
    inverse.finalize().unwrap();
    assert_eq!(
        inverse[0].get_cpu_op(false).unwrap_err().message(),
        crate::ops::lut1d::lut1d_op::NOT_PORTED_INVERSE_RENDERER
    );
}

/// A Lut1D op's queries: its type, that a copy equals it and has its cache ID, and that the
/// inverse direction makes a pair of inverses. The cache ID's text is compared with the
/// wheel's in `tests/lut1d_op_oracle.rs`.
#[test]
fn queries() {
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, create_square_lut(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, create_square_lut(), TransformDirection::Inverse);
    let copy = ops[0].clone_op().unwrap();

    assert_eq!(ops[0].get_info(), "<Lut1DOp>");
    assert!(ops[0].is_same_type(&ops[1]));
    assert!(ops[0].is_inverse(&ops[1]) && ops[1].is_inverse(&ops[0]));
    assert!(!ops[0].is_inverse(&copy));
    assert!(ops[0].data().equals(copy.data()));
    let id = ops[0].get_cache_id().unwrap();
    assert_eq!(id, copy.get_cache_id().unwrap());
    assert!(!ops[0].has_channel_crosstalk());
    assert!(!ops[0].is_no_op().unwrap());
}
