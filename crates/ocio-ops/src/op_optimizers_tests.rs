// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the optimizer, from upstream's tests/cpu/OpOptimizers_tests.cpp @ v2.5.2. The
//! optimizer is also checked against the wheel: `tests/op_optimizers_oracle.rs` (the optimized
//! lists of Matrix ops, for every level and several flags alone), and
//! `tests/cpu_processor_oracle.rs` (the debug log, the pass cap, the no-op types, the refused
//! bit depths).
//!
//! The tests of that file that aren't here need what isn't ported yet:
//! - files, through `BuildOpsTest` (a CTF or spi1d reader and `FileTransform`, Phase 4):
//!   `optimization`, `optimization2`, `lut1d_identities`, `lut1d_identity_replacement_order`,
//!   `invlut_pair_identities`, `mntr_identities`, `gamma_comp`, `gamma_comp_test2`,
//!   `log_identities`, `range_lut`, `prefer_pair_inverse_over_combine`, `opt_prefix_test1`;
//! - the ExposureContrast op (Phase 5): `dynamic_ops`, `dyn_properties_prefix`;
//! - the inverse Lut1D and the Lut1D's float renderers (Phase 2, WP 2.1), which `CompareRender`
//!   runs: `lut1d_half_domain_keep_prior_range`, `multi_op_prefix`.

use ocio_testkit::upstream::check_close;

use crate::op::{Op, OpVec};
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use crate::ops::cdl::cdl_op::create_cdl_op;
use crate::ops::cdl::{CdlOpData, CdlOpStyle, ChannelParams};
use crate::ops::exponent::exponent_op::create_exponent_op_from_values;
use crate::ops::fixedfunction::FixedFunctionOpStyle;
use crate::ops::fixedfunction::fixed_function_op::create_fixed_function_op_from_data;
use crate::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpData;
use crate::ops::gamma::gamma_op::create_gamma_op;
use crate::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use crate::ops::lut1d::Lut1DOpData;
use crate::ops::lut1d::lut1d_op::create_lut1d_op;
use crate::ops::lut1d::lut1d_op_data::HalfFlags;
use crate::ops::matrix::MatrixOpData;
use crate::ops::matrix::matrix_op::{create_matrix_offset_op, create_matrix_op, create_scale_op};
use crate::ops::range::RangeOpData;
use crate::ops::range::range_op::{create_range_op, create_range_op_from_values};

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

/// Port of `AllBut` (tests/cpu/OpOptimizers_tests.cpp:24-27 @ v2.5.2).
fn all_but(not_flag: OptimizationFlags) -> OptimizationFlags {
    OptimizationFlags(OptimizationFlags::ALL.0 & !not_flag.0)
}

/// Port of `CompareRender` (tests/cpu/OpOptimizers_tests.cpp:29-66 @ v2.5.2), without
/// `forceAlphaInRange`, which only the unported tests set. `Op::apply` renders in place,
/// as upstream's `op->apply(&img1[0], &img1[0], nbPixels)` does.
fn compare_render(ops1: &OpVec, ops2: &OpVec, error_threshold: f32) {
    let mut img1: Vec<f32> = vec![
        0.778, 0.824, 0.885, 0.153, //
        0.044, 0.014, 0.088, 0.999, //
        0.488, 0.381, 0., 0., //
        1.000, 1.52e-4, 0.0229, 1., //
        0., -0.1, -2., -0.1, //
        2., 1.9, 0., 2.,
    ];

    let mut img2 = img1.clone();

    for op in ops1.iter() {
        // NB: This hard-codes OPTIMIZATION_FAST_LOG_EXP_POW to off, see Op.h.
        op.apply(&mut img1).unwrap();
    }

    for op in ops2.iter() {
        op.apply(&mut img2).unwrap();
    }

    for idx in 0..img1.len() {
        check_close(img1[idx], img2[idx], error_threshold);
    }
}

/// The type of the data of `op`.
fn op_type(op: &Op) -> OpDataType {
    op.data().get_type()
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, remove_leading_clamp_identity)` @ v2.5.2.
#[test]
fn remove_leading_clamp_identity() {
    let mut ops = OpVec::new();

    let range = RangeOpData::with_values(0., 1., 0., 1.).unwrap();
    let range2 = RangeOpData::with_values(0., 1., 0., 2.).unwrap();
    let matrix = MatrixOpData::new();

    let fwd = TransformDirection::Forward;
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_matrix_op(&mut ops, matrix.clone(), fwd);
    assert_eq!(ops.len(), 4);
    super::remove_leading_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(op_type(&ops[0]), OpDataType::Matrix);
    ops.clear();

    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 3);
    super::remove_leading_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 0);
    ops.clear();

    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_matrix_op(&mut ops, matrix.clone(), fwd);
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 4);
    super::remove_leading_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 3);
    assert_eq!(op_type(&ops[0]), OpDataType::Matrix);
    assert_eq!(op_type(&ops[1]), OpDataType::Range);
    assert_eq!(op_type(&ops[2]), OpDataType::Range);

    ops.clear();

    // First range is not an identity, nothing to remove.
    create_range_op(&mut ops, range2.clone(), fwd).unwrap();
    create_matrix_op(&mut ops, matrix.clone(), fwd);
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 4);
    super::remove_leading_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 4);
    assert_eq!(op_type(&ops[0]), OpDataType::Range);
    assert_eq!(op_type(&ops[1]), OpDataType::Matrix);
    assert_eq!(op_type(&ops[2]), OpDataType::Range);
    assert_eq!(op_type(&ops[3]), OpDataType::Range);
    ops.clear();

    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range, fwd).unwrap();
    create_range_op(&mut ops, range2, fwd).unwrap();
    create_matrix_op(&mut ops, matrix, fwd);
    assert_eq!(ops.len(), 4);
    super::remove_leading_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(op_type(&ops[0]), OpDataType::Range);
    assert_eq!(op_type(&ops[1]), OpDataType::Matrix);
    ops.clear();
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, remove_trailing_clamp_identity)` @ v2.5.2.
#[test]
fn remove_trailing_clamp_identity() {
    let mut ops = OpVec::new();

    let range = RangeOpData::with_values(0., 1., 0., 1.).unwrap();
    let range2 = RangeOpData::with_values(0., 1., 0., 2.).unwrap();
    let matrix = MatrixOpData::new();

    let fwd = TransformDirection::Forward;
    create_matrix_op(&mut ops, matrix.clone(), fwd);
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 4);
    super::remove_trailing_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(op_type(&ops[0]), OpDataType::Matrix);
    ops.clear();

    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 3);
    super::remove_trailing_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 0);
    ops.clear();

    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_matrix_op(&mut ops, matrix.clone(), fwd);
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 4);
    super::remove_trailing_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(op_type(&ops[0]), OpDataType::Range);
    assert_eq!(op_type(&ops[1]), OpDataType::Matrix);
    ops.clear();

    // Last range is not an identity, nothing to remove.
    create_range_op(&mut ops, range2.clone(), fwd).unwrap();
    create_matrix_op(&mut ops, matrix.clone(), fwd);
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range2.clone(), fwd).unwrap();
    assert_eq!(ops.len(), 4);
    super::remove_trailing_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 4);
    assert_eq!(op_type(&ops[0]), OpDataType::Range);
    assert_eq!(op_type(&ops[1]), OpDataType::Matrix);
    assert_eq!(op_type(&ops[2]), OpDataType::Range);
    assert_eq!(op_type(&ops[3]), OpDataType::Range);
    ops.clear();

    create_range_op(&mut ops, range2, fwd).unwrap();
    create_matrix_op(&mut ops, matrix, fwd);
    create_range_op(&mut ops, range.clone(), fwd).unwrap();
    create_range_op(&mut ops, range, fwd).unwrap();
    assert_eq!(ops.len(), 4);
    super::remove_trailing_clamp_identity(&mut ops).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(op_type(&ops[0]), OpDataType::Range);
    assert_eq!(op_type(&ops[1]), OpDataType::Matrix);
    ops.clear();
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, combine_ops)` @ v2.5.2.
#[test]
fn combine_ops() {
    let m1 = [2.0, 2.0, 2.0, 1.0];
    let m2 = [0.5, 0.5, 0.5, 1.0];
    let m3 = [0.6, 0.6, 0.6, 1.0];
    let m4 = [0.7, 0.7, 0.7, 1.0];

    let exp = [1.2, 1.3, 1.4, 1.5];

    let fwd = TransformDirection::Forward;
    let inv = TransformDirection::Inverse;
    let all = OptimizationFlags::ALL;

    {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &m1, fwd);

        assert_eq!(ops.len(), 1);
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 1);
    }

    {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m3, fwd);

        assert_eq!(ops.len(), 2);
        super::combine_ops(&mut ops, all_but(OptimizationFlags::COMP_MATRIX)).unwrap();
        assert_eq!(ops.len(), 2);
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 1);
    }

    {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m3, fwd);
        create_scale_op(&mut ops, &m4, fwd);

        assert_eq!(ops.len(), 3);
        super::combine_ops(&mut ops, all_but(OptimizationFlags::COMP_MATRIX)).unwrap();
        assert_eq!(ops.len(), 3);
        super::combine_ops(&mut ops, all).unwrap();
        // CombineOps removes at most one pair on each call, repeat to combine all pairs.
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 1);
    }

    {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m2, fwd);

        assert_eq!(ops.len(), 2);
        super::combine_ops(&mut ops, all_but(OptimizationFlags::COMP_MATRIX)).unwrap();
        assert_eq!(ops.len(), 2);
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 0);
    }

    {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m1, inv);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);
        super::combine_ops(&mut ops, all_but(OptimizationFlags::COMP_MATRIX)).unwrap();
        assert_eq!(ops.len(), 2);
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 0);
    }

    {
        let mut ops = OpVec::new();
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m1, fwd);

        assert_eq!(ops.len(), 5);
        super::combine_ops(&mut ops, all_but(OptimizationFlags::COMP_MATRIX)).unwrap();
        assert_eq!(ops.len(), 5);
        super::combine_ops(&mut ops, all).unwrap();
        // CombineOps removes at most one pair on each call, repeat to combine all pairs.
        super::combine_ops(&mut ops, all).unwrap();
        super::combine_ops(&mut ops, all).unwrap();
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 1);
    }

    {
        let mut ops = OpVec::new();
        create_exponent_op_from_values(&mut ops, &exp, fwd).unwrap();
        create_scale_op(&mut ops, &m1, fwd);
        create_scale_op(&mut ops, &m2, fwd);
        create_exponent_op_from_values(&mut ops, &exp, inv).unwrap();

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 4);
        super::combine_ops(&mut ops, all_but(OptimizationFlags::COMP_MATRIX)).unwrap();
        assert_eq!(ops.len(), 4);
        super::combine_ops(&mut ops, all).unwrap();
        // CombineOps removes at most one pair on each call, repeat to combine all pairs.
        super::combine_ops(&mut ops, all).unwrap();
        assert_eq!(ops.len(), 0);
    }
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, non_optimizable)` @ v2.5.2.
#[test]
fn non_optimizable() {
    let mut ops = OpVec::new();
    // Create non identity Matrix.
    let m44 = [
        2., 0., 0., 0., //
        0., 1., 0., 0., //
        0., 0., 1., 0., //
        0., 0., 0., 1.,
    ];
    let offset4 = [0., 0., 0., 0.];
    create_matrix_offset_op(&mut ops, &m44, &offset4, TransformDirection::Forward);

    assert_eq!(ops.len(), 1);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(ops.len(), 1);

    let op = &ops[0];
    let OpData::Matrix(mat) = &**op.data() else {
        panic!("a Matrix op: {op}");
    };

    assert_eq!(mat.get_array().get_values()[0], 2.);
    assert!(mat.is_diagonal());
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, optimizable)` @ v2.5.2.
#[test]
fn optimizable() {
    let mut ops = OpVec::new();
    // Create identity Matrix.
    let mut m44 = [
        1., 0., 0., 0., //
        0., 1., 0., 0., //
        0., 0., 1., 0., //
        0., 0., 0., 1.,
    ];
    let offset4 = [0., 0., 0., 0.];
    create_matrix_offset_op(&mut ops, &m44, &offset4, TransformDirection::Forward);

    assert_eq!(ops.len(), 1);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Identity matrix is a no-op and is removed. CPU processor will re-add an identity matrix
    // if there are no ops left.
    assert_eq!(ops.len(), 0);
    ops.clear();

    // Add identity matrix.
    create_matrix_offset_op(&mut ops, &m44, &offset4, TransformDirection::Forward);

    // No more an 'identity matrix'.
    m44[0] = 2.;
    m44[1] = 2.;
    create_matrix_offset_op(&mut ops, &m44, &offset4, TransformDirection::Forward);

    assert_eq!(ops.len(), 2);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(ops.len(), 1);

    let op = &ops[0];
    let OpData::Matrix(mat) = &**op.data() else {
        panic!("a Matrix op: {op}");
    };
    assert!(!mat.is_identity().unwrap());
    assert!(!mat.is_diagonal());
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, lut1d_identity_replacement)` @ v2.5.2.
#[test]
fn lut1d_identity_replacement() {
    // Test that an identity Lut1D becomes a range but a half-domain becomes a matrix.
    {
        let lut_data = Lut1DOpData::new(3).unwrap();
        assert!(lut_data.is_identity());

        let mut ops = OpVec::new();
        create_lut1d_op(&mut ops, lut_data, TransformDirection::Forward);

        assert_eq!(ops.len(), 1);

        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 1);

        assert_eq!(ops[0].get_info(), "<RangeOp>");
    }
    {
        // By setting the filterNaNs argument to true, the constructor replaces NaN values with 0
        // and this causes the LUT to technically no longer be an identity since the values are no
        // longer exactly what is in a half float.
        let mut lut_data =
            Lut1DOpData::with_half_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, true).unwrap();
        lut_data.set_file_output_bit_depth(BitDepth::F32);
        assert!(!lut_data.is_identity());
        assert!(lut_data.is_input_half_domain());
    }
    {
        // By default, this constructor creates an 'identity lut'.
        let mut lut_data =
            Lut1DOpData::with_half_flags(HalfFlags::INPUT_OUTPUT_HALF_CODE, 65536, false).unwrap();
        lut_data.set_file_output_bit_depth(BitDepth::F32);
        assert!(lut_data.is_identity());
        assert!(lut_data.is_input_half_domain());

        let mut ops = OpVec::new();
        create_lut1d_op(&mut ops, lut_data, TransformDirection::Forward);

        assert_eq!(ops.len(), 1);

        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();

        // Half domain LUT 1d is a no-op.
        // CPU processor will add an identity matrix.
        assert_eq!(ops.len(), 0);
    }
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, range_composition)` @ v2.5.2.
#[test]
fn range_composition() {
    let empty_value = RangeOpData::empty_value();
    let fwd = TransformDirection::Forward;
    let inv = TransformDirection::Inverse;

    // Upstream's `CreateRangeOp(ops, minIn, maxIn, minOut, maxOut, dir)`, which isn't checked
    // for exceptions there.
    let create = |ops: &mut OpVec, min_in, max_in, min_out, max_out, dir| {
        create_range_op_from_values(ops, min_in, max_in, min_out, max_out, dir).unwrap();
    };

    // The part of each block after the ops are created: optimize a copy of them and compare
    // the renders.
    let check_one_range = |ops: &OpVec| {
        let mut opt_ops = ops.clone_ops().unwrap();
        opt_ops.finalize().unwrap();
        opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(opt_ops.len(), 1);
        assert_eq!(opt_ops[0].get_info(), "<RangeOp>");
        compare_render(ops, &opt_ops, 1e-6);
    };

    {
        // Two identity clamp negs ranges are collapsed into one.
        let mut ops = OpVec::new();
        create(&mut ops, 0., empty_value, 0., empty_value, fwd);
        create(&mut ops, 0., empty_value, 0., empty_value, fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        check_one_range(&ops);
    }
    {
        // Non identity ranges are combined.
        let mut ops = OpVec::new();
        create(&mut ops, 0.1, empty_value, 0.1, empty_value, fwd);
        create(&mut ops, 0.2, empty_value, 0.2, empty_value, fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        check_one_range(&ops);
    }
    {
        // Non identity ranges are combined.
        let mut ops = OpVec::new();
        create(&mut ops, 0.1, empty_value, 0.1, empty_value, fwd);
        create(&mut ops, 0.1, empty_value, 0.1, empty_value, inv);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        check_one_range(&ops);
    }
    {
        // Non identity ranges are combined.
        let mut ops = OpVec::new();
        create(&mut ops, -0.1, empty_value, -0.1, empty_value, fwd);
        create(&mut ops, -0.1, empty_value, -0.1, empty_value, inv);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        check_one_range(&ops);
    }
    {
        // A clamp negs range is dropped before a more restrictive one.
        let mut ops = OpVec::new();
        create(&mut ops, 0., empty_value, 0., empty_value, fwd);
        create(&mut ops, 0.1, 2., 0., 2., fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        check_one_range(&ops);
    }
    {
        let mut ops = OpVec::new();
        create(&mut ops, 0., 0.5, 0., 0.5, fwd);
        create(&mut ops, 0.1, 1., 0.1, 1., fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        check_one_range(&ops);
    }
    {
        let mut ops = OpVec::new();
        create(&mut ops, 0., 0.6, 0., 0.5, fwd);
        create(&mut ops, 0.1, 1., 0.2, 1., fwd);
        create(&mut ops, 0., 1., 0., 2., fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 3);

        check_one_range(&ops);
    }
    {
        let mut ops = OpVec::new();
        create(&mut ops, 0., 0.5, 0., 0.5, fwd);
        create(&mut ops, 0.6, 1., 0.6, 1., fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        // Two Ranges with non-overlapping pass regions are replaced with a clamp to a constant.
        check_one_range(&ops);
    }
    {
        let mut ops = OpVec::new();
        create(&mut ops, 0.6, 1., 0.6, 1., fwd);
        create(&mut ops, 0., 0.5, 0., 0.5, fwd);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 2);

        // Ranges can not be combined out domain of the first does not intersect
        // with in domain of the second.
        check_one_range(&ops);
    }
    {
        let mut ops = OpVec::new();
        create(&mut ops, 0., 0.6, 0., 0.5, fwd);
        create(&mut ops, 0.1, 1., 0.2, 1., fwd);
        create(&mut ops, 0.1, 1., 0.2, 1., inv);
        create(&mut ops, 0., 1., 0., 2., fwd);
        create(&mut ops, 0.1, 0.8, 0.2, 1., inv);
        create(&mut ops, 0.2, 0.6, 0.1, 0.7, inv);

        ops.finalize().unwrap();
        assert_eq!(ops.len(), 6);

        check_one_range(&ops);
    }
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, gamma_comp_identity)` @ v2.5.2.
#[test]
fn gamma_comp_identity() {
    let mut ops = OpVec::new();

    let mut params1 = vec![0.45];
    let mut params_a = vec![1.];

    let gamma1 = GammaOpData::new(
        GammaStyle::BasicFwd,
        params1.clone(),
        params1.clone(),
        params1.clone(),
        params_a.clone(),
    );

    // Note that gamma2 is not a pair inverse of gamma1, it is another FWD gamma where the
    // parameter is an inverse. Therefore it won't get replaced as a pair inverse, it must
    // be composed into an identity, which may then be replaced. Since the BASIC_FWD style
    // clamps negatives, it is replaced with a Range.
    let mut params2 = vec![1. / 0.45];

    let gamma2 = GammaOpData::new(
        GammaStyle::BasicFwd,
        params2.clone(),
        params2.clone(),
        params2.clone(),
        params_a.clone(),
    );

    create_gamma_op(&mut ops, gamma1, TransformDirection::Forward);
    create_gamma_op(&mut ops, gamma2, TransformDirection::Forward);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);

    {
        let mut opt_ops = ops.clone_ops().unwrap();

        opt_ops.finalize().unwrap();
        opt_ops
            .optimize(all_but(OptimizationFlags::IDENTITY_GAMMA))
            .unwrap();

        assert_eq!(opt_ops.len(), 1);
        assert_eq!(opt_ops[0].get_info(), "<GammaOp>");
    }
    {
        let mut opt_ops = ops.clone_ops().unwrap();

        // BASIC gammas are composed resulting in an identity, that get optimized as a range.
        opt_ops.finalize().unwrap();
        opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

        assert_eq!(opt_ops.len(), 1);
        assert_eq!(opt_ops[0].get_info(), "<RangeOp>");
    }

    // Now do the same test with MONCURVE rather than BASIC style.

    ops.clear();

    params1 = vec![2., 0.5];
    params2 = vec![2., 0.6];
    params_a = vec![1., 0.];
    let gamma1 = GammaOpData::new(
        GammaStyle::MoncurveFwd,
        params1.clone(),
        params1.clone(),
        params1,
        params_a.clone(),
    );
    let gamma2 = GammaOpData::new(
        GammaStyle::MoncurveFwd,
        params2.clone(),
        params2.clone(),
        params2,
        params_a,
    );

    create_gamma_op(&mut ops, gamma1, TransformDirection::Forward);
    create_gamma_op(&mut ops, gamma2, TransformDirection::Forward);

    ops.finalize().unwrap();
    assert_eq!(ops.len(), 2);

    let mut opt_ops = ops.clone_ops().unwrap();

    // MONCURVE composition is not supported yet.
    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(opt_ops.len(), 2);
    assert_eq!(opt_ops[0].get_info(), "<GammaOp>");
    assert_eq!(opt_ops[1].get_info(), "<GammaOp>");
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, replace_ops)` @ v2.5.2.
///
/// Upstream's ops share `cdlData`, so its later setters reach them; here each op copies it,
/// and nothing changes the copies.
#[test]
fn replace_ops() {
    let mut cdl_data = CdlOpData::new_default();
    let new_offset_params = ChannelParams::splat(0.09);
    cdl_data.set_offset_params(new_offset_params);
    cdl_data.set_saturation(1.23);
    cdl_data.set_slope_params(ChannelParams::new(0.8, 0.9, 1.1));
    cdl_data.set_style(CdlOpStyle::NoClampFwd);

    let mut original_ops = OpVec::new();

    create_cdl_op(
        &mut original_ops,
        cdl_data.clone(),
        TransformDirection::Forward,
    );
    assert_eq!(original_ops.len(), 1);

    let mut optimized_ops = original_ops.clone_ops().unwrap();

    // Verify that default optimization includes replacing ops.
    assert!(OptimizationFlags::DEFAULT.has_flag(OptimizationFlags::SIMPLIFY_OPS));

    // Optimize it: CDL is replaced by a matrix.
    optimized_ops.finalize().unwrap();
    optimized_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(optimized_ops.len(), 1);

    assert_eq!(op_type(&optimized_ops[0]), OpDataType::Matrix);

    // No optimization: keep CDL.
    optimized_ops = original_ops.clone_ops().unwrap();
    optimized_ops.finalize().unwrap();
    assert_eq!(optimized_ops.len(), 1);

    assert_eq!(op_type(&optimized_ops[0]), OpDataType::Cdl);

    // Only replace. CDL is replaced by 2 matrices, one for offset and slope, one for saturation.
    // Default optimization would combine them.
    optimized_ops = original_ops.clone_ops().unwrap();
    optimized_ops.finalize().unwrap();
    optimized_ops
        .optimize(OptimizationFlags::SIMPLIFY_OPS)
        .unwrap();
    assert_eq!(optimized_ops.len(), 2);

    assert_eq!(op_type(&optimized_ops[0]), OpDataType::Matrix);

    assert_eq!(op_type(&optimized_ops[1]), OpDataType::Matrix);

    // Use clamping style.
    cdl_data.set_style(CdlOpStyle::V1_2Fwd);

    optimized_ops.clear();

    create_cdl_op(
        &mut optimized_ops,
        cdl_data.clone(),
        TransformDirection::Forward,
    );
    assert_eq!(optimized_ops.len(), 1);

    // Optimize it: CDL replaced by 2 matrices and 2 clamps.
    optimized_ops.finalize().unwrap();
    optimized_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(optimized_ops.len(), 4);

    assert_eq!(op_type(&optimized_ops[0]), OpDataType::Matrix);

    assert_eq!(op_type(&optimized_ops[1]), OpDataType::Range);

    assert_eq!(op_type(&optimized_ops[2]), OpDataType::Matrix);

    assert_eq!(op_type(&optimized_ops[3]), OpDataType::Range);

    // With a non-identity power.
    cdl_data.set_power_params(ChannelParams::new(1., 1., 1.0001));

    optimized_ops.clear();

    create_cdl_op(&mut optimized_ops, cdl_data, TransformDirection::Forward);
    assert_eq!(optimized_ops.len(), 1);

    // Optimize it: CDL is not replaced.
    optimized_ops.finalize().unwrap();
    optimized_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(optimized_ops.len(), 1);

    assert_eq!(op_type(&optimized_ops[0]), OpDataType::Cdl);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, remove_inverse_ops)` @ v2.5.2.
#[test]
fn remove_inverse_ops_fixed_function() {
    use crate::ops::log::log_op::create_log_op_from_parameters;
    use TransformDirection::{Forward, Inverse};

    let func = FixedFunctionOpData::new(FixedFunctionOpStyle::AcesRedMod03Fwd).unwrap();

    let log_slope = [0.18, 0.18, 0.18];
    let lin_slope = [2.0, 2.0, 2.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let base = 10.0;
    let log_offset = [1.0, 1.0, 1.0];
    let log = |ops: &mut OpVec, dir| {
        create_log_op_from_parameters(
            ops,
            base,
            &log_slope,
            &log_offset,
            &lin_slope,
            &lin_offset,
            dir,
        );
    };
    let ff = |ops: &mut OpVec, dir| {
        create_fixed_function_op_from_data(ops, func.clone(), dir).unwrap();
    };

    let mut ops = OpVec::new();
    ff(&mut ops, Forward);
    log(&mut ops, Inverse);
    log(&mut ops, Forward);
    ff(&mut ops, Inverse);
    assert_eq!(ops.len(), 4);

    // Inverse + forward log are optimized as no-op then forward and inverse exponent are
    // optimized as no-op within the same call.
    super::remove_inverse_ops(&mut ops, OptimizationFlags::ALL).unwrap();
    assert_eq!(ops.len(), 0);
    ops.clear();

    ff(&mut ops, Forward);
    log(&mut ops, Forward);
    log(&mut ops, Inverse);
    ff(&mut ops, Inverse);
    assert_eq!(ops.len(), 4);

    // Forward + inverse log are optimized as a clamping range that stays between forward and
    // inverse exponents.
    super::remove_inverse_ops(&mut ops, OptimizationFlags::ALL).unwrap();
    assert_eq!(ops.len(), 3);
    assert_eq!(ops[0].get_info(), "<FixedFunctionOp>");
    assert_eq!(ops[1].get_info(), "<RangeOp>");
    assert_eq!(ops[2].get_info(), "<FixedFunctionOp>");
    ops.clear();

    ff(&mut ops, Forward);
    ff(&mut ops, Inverse);
    log(&mut ops, Inverse);
    log(&mut ops, Forward);
    ff(&mut ops, Forward);
    assert_eq!(ops.len(), 5);

    super::remove_inverse_ops(&mut ops, OptimizationFlags::ALL).unwrap();
    assert_eq!(ops.len(), 1);

    assert_eq!(ops[0].get_info(), "<FixedFunctionOp>");
}
