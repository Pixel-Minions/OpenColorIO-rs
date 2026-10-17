// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/OpOptimizers_tests.cpp @ v2.5.2 that read files
//! (`BuildOpsTest`). The others are `crates/ocio-ops/src/op_optimizers_tests.rs`.

use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_testkit::upstream::check_close;

use crate::context::Context;
use crate::fileformats::test_utils::build_ops_test;

/// Renders the same pixels through `ops1` and `ops2`, op by op in place, and checks that they
/// are within `error_threshold`; `force_alpha_in_range` sets the alpha of the last pixel to 0.
///
/// Port of `CompareRender` (tests/cpu/OpOptimizers_tests.cpp:29-65 @ v2.5.2).
#[track_caller]
fn compare_render(ops1: &OpVec, ops2: &OpVec, error_threshold: f32, force_alpha_in_range: bool) {
    let mut img1: Vec<f32> = vec![
        0.778, 0.824, 0.885, 0.153, //
        0.044, 0.014, 0.088, 0.999, //
        0.488, 0.381, 0., 0., //
        1.000, 1.52e-4, 0.0229, 1., //
        0., -0.1, -2., -0.1, //
        2., 1.9, 0., 2.,
    ];

    if force_alpha_in_range {
        img1[19] = 0.;
    }

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

/// The ops of the test file `file_name` in the direction `dir`, appended to `ops`.
fn build(ops: &mut OpVec, file_name: &str, dir: TransformDirection) {
    let context = Context::new();
    build_ops_test(ops, file_name, &context, dir).unwrap();
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, optimization2)` @ v2.5.2.
#[test]
fn optimization2() {
    // This transform has the following ops:
    // 1 Lut1D, half domain, effectively an identity
    // 2 Matrix, bit depth conversion identity
    // 3 Matrix, bit depth conversion identity
    // 4 Matrix, almost identity
    // 5 Range, clamp identity
    // 6 Lut1D, half domain, raw halfs, identity
    // 7 Lut1D, raw halfs, identity
    // 8 Matrix, not identity
    // 9 Matrix, not identity
    // 10 Lut1D, almost identity
    // 11 Lut1D, almost identity that composes to an identity with the previous one
    // 12 Lut3D, not identity
    // 13 Lut3D, not identity but composes to an identity with the previous one

    let mut ops = OpVec::new();
    build(&mut ops, "opt_test2.ctf", TransformDirection::Forward);

    // First one is the file no op.
    assert_eq!(ops.len(), 14);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();
    assert_eq!(ops.len(), 13);

    let mut opt_ops = ops.clone_ops().unwrap();
    assert_eq!(opt_ops.len(), 13);

    // No need to remove OPTIMIZATION_COMP_SEPARABLE_PREFIX because optimization is for F32.
    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::GOOD).unwrap();
    assert_eq!(opt_ops.len(), 4);

    // Op 1 is exactly an identity except for the first value which is 0.000001. Since the
    // outDepth=16i, this gets normalized by 1/65536, which puts it well under the noise
    // threshold so it's optimized it out as an identity.

    // Ops 2 & 3 are identities and removed.
    // This is op 4.
    assert_eq!(opt_ops[0].get_info(), "<MatrixOffsetOp>");
    // Op 5 is a clamp identity and removed.
    // Ops 6 & 7 are identities and are replaced by ranges, one is removed.
    assert_eq!(opt_ops[1].get_info(), "<RangeOp>");
    // This is op 8 composed with op 9.
    assert_eq!(opt_ops[2].get_info(), "<MatrixOffsetOp>");
    // Ops 10 & 11 composed become an identity, is replaced with a range, which is then
    // removed as a clamp identity
    // This is op 12 composed with op 13.  It is an identity.
    // NB: We don't try to detect Lut3DOp identities.
    assert_eq!(opt_ops[3].get_info(), "<Lut3DOp>");

    // Compare renders.
    compare_render(&ops, &opt_ops, 1e-6, false);
}
