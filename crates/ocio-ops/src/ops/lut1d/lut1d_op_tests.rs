// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOp_tests.cpp` @ v2.5.2: the tests that need no file
//! (`lut_1d_compose_with_bit_depth` reads one, Phase 4).
//!
//! Upstream's test runner reruns every test in each SIMD mode the CPU supports
//! (tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2); a test of a renderer with SIMD kernels runs
//! once per mode here, through `GetLut1DRenderer` with that mode's CPU flags (what
//! `Op::apply`'s `getCPUOp(false)` builds).

use super::*;
use crate::cpu_info::{
    CpuInfo, X86_CPU_FLAG_AVX, X86_CPU_FLAG_AVX2, X86_CPU_FLAG_AVX512, X86_CPU_FLAG_SSE2,
};
use crate::open_color_types::{BitDepth, Lut1DHueAdjust, OptimizationFlags};
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

/// Port of `OCIO_ADD_TEST(Lut1DOp, finite_value)` @ v2.5.2.
#[test]
fn finite_value() {
    let lut = create_square_lut();

    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut.clone(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, lut, TransformDirection::Inverse);
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let mut input_buffer_linearforward: [f32; 4] = [0.5, 0.6, 0.7, 0.5];
    let output_buffer_linearforward: [f32; 4] = [0.25, 0.36, 0.49, 0.5];
    ops[0].apply(&mut input_buffer_linearforward).unwrap();
    for i in 0..4 {
        check_close(
            input_buffer_linearforward[i],
            output_buffer_linearforward[i],
            1e-5f32,
        );
    }

    let input_buffer_linearinverse: [f32; 4] = [0.5, 0.6, 0.7, 0.5];
    let mut output_buffer_linearinverse: [f32; 4] = [0.25, 0.36, 0.49, 0.5];
    ops[1].apply(&mut output_buffer_linearinverse).unwrap();
    for i in 0..4 {
        check_close(
            input_buffer_linearinverse[i],
            output_buffer_linearinverse[i],
            1e-5f32,
        );
    }
}

/// Port of `OCIO_ADD_TEST(Lut1D, inverse_twice)` @ v2.5.2.
#[test]
fn inverse_twice() {
    // Make a LUT that squares the input.
    let lut = create_square_lut();

    let output_buffer_linearinverse: [f32; 4] = [0.5, 0.6, 0.7, 0.5];

    // Create inverse lut.
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut, TransformDirection::Inverse);
    assert_eq!(ops.len(), 1);

    let lut1d_input_buffer_reference: [f32; 4] = [0.25, 0.36, 0.49, 0.5];
    let mut lut1d_input_buffer_linearinverse: [f32; 4] = [0.25, 0.36, 0.49, 0.5];

    ops.finalize().unwrap();
    ops[0].apply(&mut lut1d_input_buffer_linearinverse).unwrap();
    for i in 0..4 {
        check_close(
            lut1d_input_buffer_linearinverse[i],
            output_buffer_linearinverse[i],
            1e-5f32,
        );
    }

    // Inverse the inverse.
    let OpData::Lut1D(p_lut) = &**ops[0].data() else {
        unreachable!("a Lut1D op")
    };
    let lut_data = p_lut.inverse();
    create_lut1d_op(&mut ops, lut_data, TransformDirection::Forward);
    assert_eq!(ops.len(), 2);

    // Apply the inverse.
    ops.finalize().unwrap();
    ops[1].apply(&mut lut1d_input_buffer_linearinverse).unwrap();

    // Verify we are back on the input.
    for i in 0..4 {
        check_close(
            lut1d_input_buffer_linearinverse[i],
            lut1d_input_buffer_reference[i],
            1e-5f32,
        );
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

/// The Lut1D op's parts that waited for Phase 2 work: its renderers, composing LUTs, the
/// inverse's set-up, renderers and fast forward LUT, and the replacement of a LUT and its
/// inverse.
#[test]
fn phase_2_parts_work() {
    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, create_square_lut(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, create_square_lut(), TransformDirection::Forward);
    ops.finalize().unwrap();

    ops[0].get_cpu_op(false).unwrap().expect("a renderer");
    ops[0].get_cpu_op(true).unwrap().expect("a renderer");
    let mut pixel = [0.5f32, 0.25, 0.125, 1.0];
    ops[0].apply(&mut pixel).unwrap();

    // Two LUTs that compose.
    assert!(ops[0].can_combine_with(&ops[1]).unwrap());
    let mut out = OpVec::new();
    ops[0].combine_with(&mut out, &ops[1]).unwrap();
    assert_eq!(out.len(), 1);
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);

    // An inverse LUT, rendered and replaced by its fast forward LUT.
    let mut inverse = OpVec::new();
    create_lut1d_op(
        &mut inverse,
        create_square_lut(),
        TransformDirection::Inverse,
    );
    inverse.finalize().unwrap();
    inverse[0].get_cpu_op(false).unwrap().expect("a renderer");
    inverse.optimize(OptimizationFlags::DEFAULT).unwrap();

    // A LUT and its inverse.
    let mut pair = OpVec::new();
    create_lut1d_op(&mut pair, create_square_lut(), TransformDirection::Forward);
    create_lut1d_op(&mut pair, create_square_lut(), TransformDirection::Inverse);
    pair.finalize().unwrap();
    pair.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(pair.len(), 1);
    assert_eq!(pair[0].get_info(), "<RangeOp>");
}

/// Port of `OCIO_ADD_TEST(Lut1DOp, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let mut luta = Lut1DOpData::new(3).unwrap();
    luta.get_array_mut()[0] = 0.1f32;

    let lutb = luta.clone();
    let mut lutc = luta.clone();
    lutc.get_array_mut()[0] = 0.2f32;

    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, luta.clone(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, luta, TransformDirection::Inverse);
    create_lut1d_op(&mut ops, lutb.clone(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, lutb, TransformDirection::Inverse);
    create_lut1d_op(&mut ops, lutc.clone(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, lutc, TransformDirection::Inverse);

    assert_eq!(ops.len(), 6);
    ops.finalize().unwrap();

    let op0 = &ops[0];
    let op1 = &ops[1];
    let op2 = &ops[2];
    let op3 = &ops[3];
    let op4 = &ops[4];
    let op5 = &ops[5];

    assert!(op0.is_inverse(op1));
    assert!(op2.is_inverse(op3));
    assert!(op4.is_inverse(op5));

    assert!(!op0.is_inverse(op2));
    assert!(op0.is_inverse(op3));
    assert!(op1.is_inverse(op2));
    assert!(!op1.is_inverse(op3));

    assert!(!op0.is_inverse(op4));
    assert!(!op0.is_inverse(op5));
    assert!(!op1.is_inverse(op4));
    assert!(!op1.is_inverse(op5));

    let cache_id0 = ops[0].get_cache_id().unwrap();
    let cache_id1 = ops[1].get_cache_id().unwrap();
    let cache_id2 = ops[2].get_cache_id().unwrap();
    let cache_id3 = ops[3].get_cache_id().unwrap();
    let cache_id4 = ops[4].get_cache_id().unwrap();
    let cache_id5 = ops[5].get_cache_id().unwrap();
    assert_eq!(cache_id0, cache_id2);
    assert_eq!(cache_id1, cache_id3);

    assert_ne!(cache_id0, cache_id4);
    assert_ne!(cache_id0, cache_id5);
    assert_ne!(cache_id1, cache_id4);
    assert_ne!(cache_id1, cache_id5);

    // Optimize will remove LUT forward and inverse (0+1, 2+3 and 4+5)
    // and replace them by a clamping range.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<RangeOp>");
}

/// Port of `OCIO_ADD_TEST(Lut1DRenderer, finite_value_hue_adjust)` @ v2.5.2.
#[test]
fn finite_value_hue_adjust() {
    // Make a LUT that squares the input.
    let mut lut_data = create_square_lut();
    lut_data.set_hue_adjust(Lut1DHueAdjust::Dw3).unwrap();

    lut_data.finalize().unwrap();
    let lut = Op::new(OpData::Lut1D(lut_data.clone()));
    assert!(!lut.is_identity().unwrap());

    let output_buffer_linearforward: [f32; 4] = [
        0.25, 0.37000, // (Hue adj modifies green here.)
        0.49, 0.5,
    ];
    let mut lut1d_input_buffer_linearforward: [f32; 4] = [0.5, 0.6, 0.7, 0.5];

    lut.apply(&mut lut1d_input_buffer_linearforward).unwrap();
    for i in 0..4 {
        check_close(
            lut1d_input_buffer_linearforward[i],
            output_buffer_linearforward[i],
            1e-5f32,
        );
    }

    let inv_data = lut_data.inverse();
    let inv_data_exact = inv_data.clone();

    let mut ops_fast = OpVec::new();
    let mut ops_exact = OpVec::new();
    create_lut1d_op(&mut ops_fast, inv_data, TransformDirection::Forward);
    create_lut1d_op(&mut ops_exact, inv_data_exact, TransformDirection::Forward);

    assert_eq!(ops_fast.len(), 1);
    assert_eq!(ops_exact.len(), 1);

    let input_buffer_linearinverse: [f32; 4] = [0.5, 0.6, 0.7, 0.5];
    let mut lut1d_output_buffer_linearinverse: [f32; 4] = [0.25, 0.37, 0.49, 0.5];
    let mut lut1d_output_buffer_linearinverse_ex: [f32; 4] = [0.25, 0.37, 0.49, 0.5];

    ops_fast.finalize().unwrap();
    ops_fast.optimize(OptimizationFlags::LUT_INV_FAST).unwrap();

    ops_exact.finalize().unwrap(); // No optimizations.

    assert_eq!(ops_fast.len(), 1);
    assert_eq!(ops_exact.len(), 1);

    let OpData::Lut1D(lut_fast) = &**ops_fast[0].data() else {
        panic!("a Lut1D op")
    };
    assert_eq!(lut_fast.get_direction(), TransformDirection::Forward);

    let OpData::Lut1D(lut_exact) = &**ops_exact[0].data() else {
        panic!("a Lut1D op")
    };
    assert_eq!(lut_exact.get_direction(), TransformDirection::Inverse);

    ops_fast[0]
        .apply(&mut lut1d_output_buffer_linearinverse)
        .unwrap(); // fast
    ops_exact[0]
        .apply(&mut lut1d_output_buffer_linearinverse_ex)
        .unwrap(); // exact
    for i in 0..4 {
        check_close(
            lut1d_output_buffer_linearinverse[i],
            input_buffer_linearinverse[i],
            1e-5f32,
        );
        check_close(
            lut1d_output_buffer_linearinverse_ex[i],
            input_buffer_linearinverse[i],
            1e-5f32,
        );
    }
}

/// Port of `OCIO_ADD_TEST(Lut1DOpData, compose_only_forward)` @ v2.5.2.
#[test]
fn compose_only_forward() {
    let l1 = create_square_lut();

    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, l1.clone(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, l1.clone(), TransformDirection::Forward);
    create_lut1d_op(&mut ops, l1.clone(), TransformDirection::Inverse);
    create_lut1d_op(&mut ops, l1, TransformDirection::Inverse);

    assert_eq!(ops.len(), 4);
    let l1f = &ops[1];
    let l1b = &ops[3];

    // Forward + forward.
    assert!(ops[0].can_combine_with(l1f).unwrap());
    // Inverse + inverse.
    assert!(ops[2].can_combine_with(l1b).unwrap());
    // Forward + Inverse.
    assert!(ops[0].can_combine_with(l1b).unwrap());
    // Inverse + forward.
    assert!(ops[2].can_combine_with(l1f).unwrap());
}

/// Port of `OCIO_ADD_TEST(Lut1D, compose_big_domain)` @ v2.5.2.
#[test]
fn compose_big_domain() {
    let mut lut1 = Lut1DOpData::new(10).unwrap();
    let lut2 = Lut1DOpData::new(10).unwrap();
    lut1.get_array_mut()[9 * 3] = 1.0001f32;

    let mut ops = OpVec::new();
    create_lut1d_op(&mut ops, lut1, TransformDirection::Forward);
    create_lut1d_op(&mut ops, lut2, TransformDirection::Forward);

    assert_eq!(ops.len(), 2);

    let op0 = ops[0].clone();
    let op1 = ops[1].clone();
    op0.combine_with(&mut ops, &op1).unwrap();
    assert_eq!(ops.len(), 3);

    let OpData::Lut1D(lut) = &**ops[2].data() else {
        panic!("a Lut1D op")
    };
    assert_eq!(lut.get_array().get_length(), 65536);
    assert!(!lut.is_input_half_domain());
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
