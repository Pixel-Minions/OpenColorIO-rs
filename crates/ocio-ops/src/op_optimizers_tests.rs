// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the optimizer, from upstream's tests/cpu/OpOptimizers_tests.cpp @ v2.5.2. The
//! others of that file come as their ops' factories are ported; meanwhile the optimizer is also
//! checked against the wheel: `tests/op_optimizers_oracle.rs` (the optimized lists of Matrix
//! ops, for every level and several flags alone), and `tests/cpu_processor_oracle.rs` (the
//! debug log, the pass cap, the no-op types, the refused bit depths).
//!
//! `gamma_prefix` is ported; `multi_op_prefix` compares the baked LUT's float rendering with
//! the original ops (`CompareRender`), which needs the Lut1D's float renderers (Phase 2), and
//! `opt_prefix_test1` reads a CTF file (the reader is WP 1.6).

use crate::op::OpVec;
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use crate::ops::gamma::gamma_op::create_gamma_op;
use crate::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};

/// Port of `OCIO_ADD_TEST(OpOptimizers, gamma_prefix)` @ v2.5.2.
#[test]
fn gamma_prefix() {
    let mut original_ops = OpVec::new();

    let params1 = vec![2.6];
    let params_a = vec![1.];

    let gamma1 = GammaOpData::new(
        GammaStyle::BasicRev,
        params1.clone(),
        params1.clone(),
        params1.clone(),
        params_a.clone(),
    );

    create_gamma_op(&mut original_ops, gamma1, TransformDirection::Forward);
    assert_eq!(original_ops.len(), 1);

    let mut optimized_ops = original_ops.clone_ops().unwrap();

    // Optimize it.
    optimized_ops.finalize().unwrap();
    optimized_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    optimized_ops
        .optimize_for_bitdepth(
            BitDepth::Uint16,
            BitDepth::F32,
            OptimizationFlags::COMP_SEPARABLE_PREFIX,
        )
        .unwrap();

    // Validate the result.

    assert_eq!(optimized_ops.len(), 1);

    let OpData::Lut1D(o_data1) = &**optimized_ops[0].data() else {
        panic!("a Lut1D op: {}", optimized_ops[0]);
    };
    assert_eq!(o_data1.get_type(), OpDataType::Lut1D);
    assert_eq!(o_data1.get_array().get_length(), 65536);
    original_ops = OpVec::new();

    // However, if the input bit depth is F32, it should not be optimized.

    let gamma2 = GammaOpData::new(
        GammaStyle::BasicRev,
        params1.clone(),
        params1.clone(),
        params1,
        params_a,
    );

    create_gamma_op(&mut original_ops, gamma2, TransformDirection::Forward);
    assert_eq!(original_ops.len(), 1);

    // Optimize it.
    original_ops.finalize().unwrap();
    original_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(original_ops.len(), 1);
    assert_eq!(original_ops[0].data().get_type(), OpDataType::Gamma);
}
