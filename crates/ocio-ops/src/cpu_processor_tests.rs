// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/CPUProcessor_tests.cpp` @ v2.5.2: so far `flag_composition`. The other
//! tests build processors from transforms through a config (WP 1.8, Phase 3), so the CPU
//! processor's own tests follow, and `tests/cpu_processor_oracle.rs` checks it against the wheel.

use crate::open_color_types::{
    BitDepth, DynamicPropertyType, OptimizationFlags, TransformDirection,
};

/// Port of `OCIO_ADD_TEST(CPUProcessor, flag_composition)` @ v2.5.2.
#[test]
fn flag_composition() {
    // The test validates the build of a custom optimization flag.

    let mut custom_flags = OptimizationFlags::LOSSLESS;

    assert_eq!(
        custom_flags & OptimizationFlags::COMP_LUT1D,
        OptimizationFlags::NONE
    );

    custom_flags = custom_flags | OptimizationFlags::COMP_LUT1D;

    assert_eq!(
        custom_flags & OptimizationFlags::COMP_LUT1D,
        OptimizationFlags::COMP_LUT1D
    );
}

/// The processor of one matrix op.
fn matrix_processor(
    mat: crate::ops::matrix::MatrixOpData,
    input: BitDepth,
    output: BitDepth,
) -> super::CpuProcessor {
    let mut ops = crate::op::OpVec::new();
    crate::ops::matrix::matrix_op::create_matrix_op(&mut ops, mat, TransformDirection::Forward);
    ops.finalize().unwrap();
    super::CpuProcessor::new(&ops, input, output, OptimizationFlags::DEFAULT).unwrap()
}

#[test]
fn an_identity_is_a_no_op_only_between_equal_bit_depths() {
    use crate::ops::matrix::MatrixOpData;
    let same = matrix_processor(
        MatrixOpData::create_diagonal_matrix(1.0),
        BitDepth::Uint8,
        BitDepth::Uint8,
    );
    assert!(same.is_identity());
    assert!(same.is_no_op());
    assert!(!same.has_channel_crosstalk());

    let different = matrix_processor(
        MatrixOpData::create_diagonal_matrix(1.0),
        BitDepth::Uint8,
        BitDepth::F32,
    );
    assert!(different.is_identity());
    assert!(!different.is_no_op());
    assert_eq!(different.get_input_bit_depth(), BitDepth::Uint8);
    assert_eq!(different.get_output_bit_depth(), BitDepth::F32);

    let mut mixing = MatrixOpData::create_diagonal_matrix(2.0);
    mixing.set_array_value(1, 0.5);
    let mixing = matrix_processor(mixing, BitDepth::F32, BitDepth::F32);
    assert!(!mixing.is_identity());
    assert!(mixing.has_channel_crosstalk());
}

#[test]
fn a_matrix_processor_has_no_dynamic_property() {
    let cpu = matrix_processor(
        crate::ops::matrix::MatrixOpData::create_diagonal_matrix(2.0),
        BitDepth::F16,
        BitDepth::Uint10,
    );
    assert!(!cpu.is_dynamic());
    assert!(!cpu.has_dynamic_property(DynamicPropertyType::Exposure));
    // The message of upstream's `CPUProcessor, dynamic_properties` test.
    assert_eq!(
        cpu.get_dynamic_property(DynamicPropertyType::Exposure)
            .unwrap_err()
            .message(),
        "Cannot find dynamic property; not used by CPU processor."
    );
}
