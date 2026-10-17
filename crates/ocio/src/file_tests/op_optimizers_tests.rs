// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/OpOptimizers_tests.cpp @ v2.5.2 that read files
//! (`BuildOpsTest`). The others are `crates/ocio-ops/src/op_optimizers_tests.rs`.

use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{NegativeStyle, OptimizationFlags, TransformDirection};
use ocio_ops::ops::exponent::exponent_op::create_exponent_op_from_values;
use ocio_testkit::upstream::check_close;

use crate::context::Context;
use crate::fileformats::test_utils::build_ops_test;
use crate::transform::Transform;
use crate::transforms::exponent_with_linear_transform::create_gamma_transform;
use crate::transforms::group_transform::GroupTransform;

/// Every optimization but `not_flag`.
///
/// Port of `AllBut` (tests/cpu/OpOptimizers_tests.cpp:24-27 @ v2.5.2).
fn all_but(not_flag: OptimizationFlags) -> OptimizationFlags {
    OptimizationFlags(OptimizationFlags::ALL.0 & !not_flag.0)
}

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

/// Port of `OCIO_ADD_TEST(OpOptimizers, prefer_pair_inverse_over_combine)` @ v2.5.2.
#[test]
fn prefer_pair_inverse_over_combine() {
    // When a pair of forward / inverse LUTs with non 0 to 1 domain are used as process space
    // for a Look (eg. CDL), the Optimizer tries to combine them when the Look results in a
    // no-op. Here we make sure this result in an appropriate clamp instead of a new
    // half-domain LUT resulting from the naive composition of the two LUTs.

    let mut ops = OpVec::new();

    // This spi1d uses "From -1.0 2.0", so the forward direction would become a Matrix to do
    // the scaling followed by a Lut1D, and the inverse is a Lut1D followed by a Matrix. Note
    // that although the matrices compose into an identity, they are both forward direction
    // and *not* pair inverses of each other.
    let file_name = "lut1d_4.spi1d";
    build(&mut ops, file_name, TransformDirection::Inverse);

    // The default negativeStyle is basicPassThruFwd, hence this op will be removed as a
    // no-op on the first optimization pass.
    let exp_null = [1.0, 1.0, 1.0, 1.0];
    create_exponent_op_from_values(&mut ops, &exp_null, TransformDirection::Forward).unwrap();

    build(&mut ops, file_name, TransformDirection::Forward);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::ALL).unwrap();
    // The starting list of ops is this:
    //     FileNoOp --> Lut1D --> Matrix --> Gamma --> FileNoOp --> Matrix --> Lut1D
    // This becomes the following on the first pass after no-ops are removed:
    //     Lut1D --> Matrix --> Matrix --> Lut1D
    // The matrices are combined and removed on the first pass, leaving this:
    //     Lut1D --> Lut1D
    // Second pass: the LUTs are identified as a pair of inverses and replaced with a Range:
    //     Range

    assert_eq!(ops.len(), 1);
    assert!(matches!(&**ops[0].data(), OpData::Range(_)));
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, optimization)` @ v2.5.2.
#[test]
fn optimization() {
    // This is a transform consisting of a Lut1d, Matrix, Matrix, Lut1d. The matrices and luts
    // are inverses of one another, so when they are composed they become identities which
    // are then replaced. So this one test actually tests quite a lot of the optimize and
    // compose functionality.

    let mut ops = OpVec::new();
    build(&mut ops, "opt_test1.ctf", TransformDirection::Forward);

    // First one is the file no op.
    assert_eq!(ops.len(), 5);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();
    assert_eq!(ops.len(), 4);

    let mut opt_ops = ops.clone_ops().unwrap();
    assert_eq!(opt_ops.len(), 4);
    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(opt_ops.len(), 1);
    assert_eq!(opt_ops[0].get_info(), "<RangeOp>");

    // Compare renders.
    compare_render(&ops, &opt_ops, 1e-6, false);
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

/// Port of `OCIO_ADD_TEST(OpOptimizers, lut1d_identities)` @ v2.5.2.
#[test]
fn lut1d_identities() {
    // This transform has the following ops:
    // 1 Lut1D, identity
    // 2 Lut1D, identity
    // 3 Lut1D, not quite identity
    // 4 Lut1D, half-domain identity (note 16i outDepth)
    // 5 Lut1D, half-domain not an identity (values clamped due to rawHalfs encoding)
    // 6 Lut1D, identity

    let mut ops = OpVec::new();
    build(
        &mut ops,
        "lut1d_identity_test.ctf",
        TransformDirection::Forward,
    );

    // First one is the file no op.
    assert_eq!(ops.len(), 7);

    assert!(ops[1].is_identity().unwrap());
    assert!(ops[2].is_identity().unwrap());
    assert!(!ops[3].is_identity().unwrap());
    assert!(ops[4].is_identity().unwrap());
    assert!(!ops[5].is_identity().unwrap());
    assert!(ops[6].is_identity().unwrap());

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    let mut opt_ops = ops.clone_ops().unwrap();
    assert_eq!(opt_ops.len(), 6);

    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(opt_ops.len(), 2);

    // The first two LUTs should get detected as identities and replaced with ranges which
    // then get removed as clamp identities. LUT 3 is not identity and is not removed. The next
    // LUT should also get detected as an identity, and replaced with a matrix (rather than a
    // range since it is half-domain) which is then optimized out. The next LUT is almost an
    // identity except the large values are clamped due to the rawHalfs encoding, so it is not
    // removed. The final LUT is a normal domain to be replaced with a range. The two Luts get
    // combined into a single Lut with a standard domain.

    assert_eq!(opt_ops[0].get_info(), "<Lut1DOp>");
    assert_eq!(opt_ops[1].get_info(), "<RangeOp>");

    let OpData::Lut1D(lut_data) = &**opt_ops[0].data() else {
        panic!("not a Lut1D");
    };
    assert_eq!(lut_data.get_array().get_length(), 65536);
    assert!(!lut_data.is_input_half_domain());

    // Now check that the optimized transform renders the same as the original.
    // TODO: Shall investigate why this test requires a bigger error.
    compare_render(&ops, &opt_ops, 3e-4, false);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, lut1d_identity_replacement_order)` @ v2.5.2.
#[test]
fn lut1d_identity_replacement_order() {
    // See issue #1737, https://github.com/AcademySoftwareFoundation/OpenColorIO/issues/1737.

    // This CTF contains a single LUT1D, inverse direction, normal (not half) domain. It
    // contains values from -6 to +3.4.
    let file_name = "lut1d_inverse_gpu.ctf";

    let mut inv_ops = OpVec::new();
    // FWD direction simply means don't swap the direction, the file contains an inverse
    // LUT1D and leave it that way.
    build(&mut inv_ops, file_name, TransformDirection::Forward);
    let mut fwd_ops = OpVec::new();
    build(&mut fwd_ops, file_name, TransformDirection::Inverse);

    // Check forward LUT1D followed by inverse LUT1D.
    {
        let mut fwd_inv_ops = fwd_ops.clone();
        fwd_inv_ops.append(&inv_ops).unwrap();

        fwd_inv_ops.finalize().unwrap();
        fwd_inv_ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(fwd_inv_ops.len(), 2); // no optmization was done

        let mut opt_ops = fwd_inv_ops.clone_ops().unwrap();
        opt_ops.finalize().unwrap();
        opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(opt_ops.len(), 1);
        assert_eq!(opt_ops[0].get_info(), "<RangeOp>");

        // Compare renders.
        compare_render(&fwd_inv_ops, &opt_ops, 1e-6, false);
    }

    // Check inverse LUT1D followed by forward LUT1D.
    {
        let mut inv_fwd_ops = inv_ops.clone();
        inv_fwd_ops.append(&fwd_ops).unwrap();

        inv_fwd_ops.finalize().unwrap();
        inv_fwd_ops.optimize(OptimizationFlags::NONE).unwrap();
        assert_eq!(inv_fwd_ops.len(), 2); // no optmization was done

        let mut opt_ops = inv_fwd_ops.clone_ops().unwrap();
        opt_ops.finalize().unwrap();
        opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(opt_ops.len(), 1);
        assert_eq!(opt_ops[0].get_info(), "<RangeOp>");

        // Compare renders.
        compare_render(&inv_fwd_ops, &opt_ops, 1e-6, false);
    }
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, invlut_pair_identities)` @ v2.5.2.
#[test]
fn invlut_pair_identities() {
    // The file contains an InverseLUT1D and LUT1D, both with the same array, followed by an
    // InverseLUT3D and LUT3D, also both with the same array. The pairs should each get
    // replaced by a range and then the ranges should be combined.
    let mut ops = OpVec::new();
    build(&mut ops, "lut_inv_pairs.ctf", TransformDirection::Forward);

    // First one is the file no op.
    assert_eq!(ops.len(), 5);

    // Remove no ops & finalize for computation.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    assert_eq!(ops.len(), 4);

    let mut opt_ops = ops.clone_ops().unwrap();
    assert_eq!(opt_ops.len(), 4);
    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    assert_eq!(opt_ops.len(), 1);
    assert_eq!(opt_ops[0].get_info(), "<RangeOp>");
    // (A range that clamps on either side is not a no-op.)
    assert!(!opt_ops[0].is_no_op().unwrap());

    // Now check that the optimized transform renders the same as the original.
    compare_render(&ops, &opt_ops, 1e-6, false);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, mntr_identities)` @ v2.5.2.
#[test]
fn mntr_identities() {
    // Forward and inverse monitor transforms should become an identity.
    let mut ops = OpVec::new();
    build(
        &mut ops,
        "mntr_srgb_identity.ctf",
        TransformDirection::Forward,
    );

    // First one is the file no op.
    assert_eq!(ops.len(), 5);

    // Remove no ops & finalize for computation.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    assert_eq!(ops.len(), 4);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Identity transform.
    assert_eq!(ops.len(), 0);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, gamma_comp)` @ v2.5.2.
#[test]
fn gamma_comp() {
    // This transform has a pair of gammas separated by an identity matrix that should
    // compose into a single (non-identity) gamma that then should be identified as a pair
    // identity with another gamma and be replaced with a clamp-negs range.

    let mut ops = OpVec::new();
    build(&mut ops, "gamma_comp_test.ctf", TransformDirection::Forward);

    // First one is the file no op.
    assert_eq!(ops.len(), 5);

    // Remove no ops & finalize for computation.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    assert_eq!(ops.len(), 4);

    let mut opt_ops = ops.clone_ops().unwrap();
    let mut opt_ops_no_comp = ops.clone_ops().unwrap();

    assert_eq!(opt_ops_no_comp.len(), 4);
    opt_ops_no_comp.finalize().unwrap();
    opt_ops_no_comp
        .optimize(all_but(OptimizationFlags::COMP_GAMMA))
        .unwrap();
    // Identity matrix is removed but gamma are not combined.
    assert_eq!(opt_ops_no_comp.len(), 3);
    assert_eq!(opt_ops_no_comp[0].get_info(), "<GammaOp>");
    assert_eq!(opt_ops_no_comp[1].get_info(), "<GammaOp>");
    assert_eq!(opt_ops_no_comp[2].get_info(), "<GammaOp>");

    compare_render(&ops, &opt_ops_no_comp, 1e-6, false);

    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Identity matrix is removed and gammas combined and optimized as a range.
    assert_eq!(opt_ops.len(), 1);
    assert_eq!(opt_ops[0].get_info(), "<RangeOp>");

    // Now check that the optimized transform renders the same as the original.
    // TODO: Gamma is clamping alpha, and Range does not.
    compare_render(&ops, &opt_ops, 1e-4, true);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, gamma_comp_test2)` @ v2.5.2.
#[test]
fn gamma_comp_test2() {
    // This transform has a pair of gammas separated by a pair of matrices that compose into
    // an identity matrix and get optimized out. Then the gammas get composed into a
    // non-identity gamma. Finally the exponent is inverted (to follow the convention of
    // keeping it > 1) and the direction is inverted.

    let mut ops = OpVec::new();
    build(
        &mut ops,
        "gamma_comp_test2.ctf",
        TransformDirection::Forward,
    );

    // First one is the file no op.
    assert_eq!(ops.len(), 5);

    // Remove no ops & finalize for computation.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    assert_eq!(ops.len(), 4);

    let mut opt_ops = ops.clone_ops().unwrap();
    let mut opt_ops_no_comp = ops.clone_ops().unwrap();

    assert_eq!(opt_ops_no_comp.len(), 4);
    opt_ops_no_comp.finalize().unwrap();
    // NB: The op->apply function used here hard-codes OPTIMIZATION_FAST_LOG_EXP_POW to off.
    opt_ops_no_comp
        .optimize(all_but(OptimizationFlags::COMP_GAMMA))
        .unwrap();
    assert_eq!(opt_ops_no_comp.len(), 2);
    assert_eq!(opt_ops_no_comp[0].get_info(), "<GammaOp>");
    assert_eq!(opt_ops_no_comp[1].get_info(), "<GammaOp>");

    // Due to rounding error in the two 3x3 matrix multiplies with much larger values, the
    // 1.52e-4 input value is off by 60% going into the second gamma (see ociochecklut -s).
    // Therefore the optOps_noComp and optOps are actually more accurate than ops here.
    compare_render(&ops, &opt_ops_no_comp, 1e-4, false);

    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    // Now check that the optimized transform renders the same as the original.
    compare_render(&ops, &opt_ops, 1e-4, false);

    // Check the op is as expected.
    let mut group = GroupTransform::new();
    assert_eq!(opt_ops.len(), 1);
    create_gamma_transform(&mut group, &opt_ops[0]).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::Exponent(g_transform) = transform else {
        panic!("not an ExponentTransform: {transform:?}");
    };
    assert_eq!(g_transform.negative_style(), NegativeStyle::PassThru);
    assert_eq!(g_transform.direction(), TransformDirection::Inverse);
    let vals = g_transform.value();
    check_close(vals[0], 2.2 / 1.8, f64::from(1e-6f32));
    check_close(vals[1], 2.2 / 1.8, f64::from(1e-6f32));
    check_close(vals[2], 2.2 / 1.8, f64::from(1e-6f32));
    assert_eq!(vals[3], 1.);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, log_identities)` @ v2.5.2.
#[test]
fn log_identities() {
    // Log fwd and rev transforms should become a range. This transform has two pair of
    // LogOps separated by an identity matrix that should optimize into a range.

    let mut ops = OpVec::new();
    build(&mut ops, "log_identities.ctf", TransformDirection::Forward);

    // First one is the file no op.
    assert_eq!(ops.len(), 6);

    // Remove no ops & finalize for computation.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    assert_eq!(ops.len(), 5);

    let mut opt_ops = ops.clone_ops().unwrap();
    let mut opt_ops_off = ops.clone_ops().unwrap();

    assert_eq!(opt_ops_off.len(), 5);
    opt_ops_off.finalize().unwrap();
    opt_ops_off
        .optimize(all_but(OptimizationFlags::PAIR_IDENTITY_LOG))
        .unwrap();
    // Only the identity matrix is optimized.
    assert_eq!(opt_ops_off.len(), 4);

    assert_eq!(opt_ops.len(), 5);
    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    // Identity matrix is optimized and forward/inverse log are combined.
    assert_eq!(opt_ops.len(), 1);

    // (A range that clamps on either side is not a no-op.)
    assert!(!opt_ops[0].is_no_op().unwrap());
    assert_eq!(opt_ops[0].get_info(), "<RangeOp>");

    // Now check that the optimized transform renders the same as the original.
    compare_render(&ops, &opt_ops, 1e-4, false);
}

/// Port of `OCIO_ADD_TEST(OpOptimizers, range_lut)` @ v2.5.2.
#[test]
fn range_lut() {
    // Non-identity range before a Lut1D should not be removed.

    let mut ops = OpVec::new();
    build(&mut ops, "range_lut.ctf", TransformDirection::Forward);

    // First one is the file no op.
    assert_eq!(ops.len(), 3);

    // Remove no ops & finalize for computation.
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();

    assert_eq!(ops.len(), 2);

    let mut opt_ops = ops.clone_ops().unwrap();
    assert_eq!(opt_ops.len(), 2);
    opt_ops.finalize().unwrap();
    opt_ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(opt_ops.len(), 2);
    assert_eq!(opt_ops[0].get_info(), "<RangeOp>");
    assert_eq!(opt_ops[1].get_info(), "<Lut1DOp>");

    // Now check that the optimized transform renders the same as the original.
    compare_render(&ops, &opt_ops, 1e-6, false);
}
