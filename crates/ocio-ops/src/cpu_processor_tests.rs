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

/// A matrix processor has no dynamic property, and asking for one gives upstream's message.
/// `is_dynamic` is checked against the wheel in `tests/cpu_processor_oracle.rs`
/// (`is_dynamic_matches_the_wheel`).
///
/// Hand-derived: the oracle doesn't expose hasDynamicProperty; pinned in p1-processor.
#[test]
fn a_matrix_processor_has_no_dynamic_property() {
    let cpu = matrix_processor(
        crate::ops::matrix::MatrixOpData::create_diagonal_matrix(2.0),
        BitDepth::F16,
        BitDepth::Uint10,
    );
    assert!(!cpu.has_dynamic_property(DynamicPropertyType::Exposure));
    // The message of upstream's `CPUProcessor, dynamic_properties` test.
    assert_eq!(
        cpu.get_dynamic_property(DynamicPropertyType::Exposure)
            .unwrap_err()
            .message(),
        "Cannot find dynamic property; not used by CPU processor."
    );
}
