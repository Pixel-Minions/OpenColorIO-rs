// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of upstream's tests/cpu/ops/lut3d/Lut3DOp_tests.cpp @ v2.5.2 that read files
//! (`BuildOpsTest`). The others are `crates/ocio-ops/src/ops/lut3d/lut3d_op_tests.rs` and
//! `crates/ocio/src/transforms/lut3d_transform_tests.rs`.

use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut3d::lut3d_op::create_lut3d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::{
    Interpolation, Lut3DOpData, make_fast_lut3d_from_inverse,
};
use ocio_testkit::upstream::check_close;

use crate::context::Context;
use crate::fileformats::test_utils::build_ops_test;

/// The ops of the test file `file_name`, forward.
fn build(file_name: &str) -> OpVec {
    let mut ops = OpVec::new();
    let context = Context::new();
    build_ops_test(&mut ops, file_name, &context, TransformDirection::Forward).unwrap();
    ops
}

/// The Lut3D data of `op`.
fn lut3d_data(op: &Op) -> &Lut3DOpData {
    let OpData::Lut3D(lut) = &**op.data() else {
        panic!("not a Lut3D");
    };
    lut
}

/// `Lut3DOp op(data)`: an op of `data`, forward.
fn lut3d_op(data: Lut3DOpData) -> Op {
    let mut ops = OpVec::new();
    create_lut3d_op(&mut ops, data, TransformDirection::Forward);
    ops[0].clone()
}

/// The pixels the renderer tests start from.
const IN_IMAGE: [f32; 12] = [
    0.1, 0.25, 0.7, 0.0, //
    0.66, 0.25, 0.81, 0.5, //
    // 0.18f, 0.80f, 0.45f, 1.f }; // This one is easier to get correct.
    0.18, 0.99, 0.45, 1.0, // Setting G way up on the s-curve is harder.
];

/// Port of `OCIO_ADD_TEST(Lut3DOp, cpu_renderer_cloned)` @ v2.5.2.
#[test]
fn cpu_renderer_cloned() {
    // The unit test validates the processing of cloned ops.

    let mut ops = build("clf/lut3d_17x17x17_10i_12i.clf");

    assert_eq!(2, ops.len());
    let lut_data = lut3d_data(&ops[1]);
    assert_eq!(lut_data.get_file_output_bit_depth(), BitDepth::Uint12);

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();
    assert_eq!(1, ops.len());

    let fwd_lut_data = lut3d_data(&ops[0]);
    assert_eq!(fwd_lut_data.get_file_output_bit_depth(), BitDepth::Uint12);

    let mut fwd_lut = ops[0].clone();

    let fwd_lut_data_cloned = fwd_lut_data.clone();
    let mut fwd_lut_cloned = lut3d_op(fwd_lut_data_cloned);

    let mut fwd_lut_cloned_cloned = fwd_lut_cloned.clone_op().unwrap();

    let mut buffer_image = IN_IMAGE;
    let mut buffer_image_clone = IN_IMAGE;
    let mut buffer_image_clone2 = IN_IMAGE;

    // Apply the forward LUT.
    fwd_lut.finalize().unwrap();
    fwd_lut.apply(&mut buffer_image).unwrap();

    // Apply the cloned forward LUT.
    fwd_lut_cloned.finalize().unwrap();
    fwd_lut_cloned.apply(&mut buffer_image_clone).unwrap();

    // Apply the cloned cloned forward LUT.
    fwd_lut_cloned_cloned.finalize().unwrap();
    fwd_lut_cloned_cloned
        .apply(&mut buffer_image_clone2)
        .unwrap();

    // Validate that the cloned op produces the exact same results.
    for i in 0..12 {
        assert_eq!(buffer_image_clone[i], buffer_image[i]);
        assert_eq!(buffer_image_clone2[i], buffer_image[i]);
    }
}

/// Port of `OCIO_ADD_TEST(Lut3DOp, cpu_renderer_inverse)` @ v2.5.2.
#[test]
fn cpu_renderer_inverse() {
    // The unit test validates the processing of inversed ops.

    let mut ops = build("clf/lut3d_17x17x17_10i_12i.clf");

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();

    let fwd_lut_data = lut3d_data(&ops[0]);
    let mut fwd_lut_data_cloned = fwd_lut_data.clone();
    // Inversion is based on tetrahedral interpolation, so need to make sure the forward evals
    // are also tetrahedral.
    fwd_lut_data_cloned.set_interpolation(Interpolation::Tetrahedral);

    let mut fwd_lut = lut3d_op(fwd_lut_data_cloned.clone());

    let mut buffer_image = IN_IMAGE;

    // Apply forward LUT.
    fwd_lut.validate().unwrap();
    fwd_lut.finalize().unwrap();
    fwd_lut.apply(&mut buffer_image).unwrap();

    //
    // Step 1: Test that forward and inverse ops are producing the right results in EXACT
    //         mode.
    //

    let out_image1 = buffer_image;

    let inv_lut_data = fwd_lut_data_cloned.inverse();
    let mut inv_lut = lut3d_op(inv_lut_data.clone());

    // Apply inverse LUT.
    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();
    inv_lut.apply(&mut buffer_image).unwrap();

    // Need to do another forward apply. This is due to precision issues. Also, some LUTs have
    // flat or virtually flat areas so the inverse is not unique. The first inverse does not
    // match the source, but the fact that it winds up in the same place after another cycle
    // verifies that it is as good an inverse for this particular LUT as the original input.
    // In other words, when the forward LUT has a small derivative, precision issues imply that
    // there will be a range of inverses which should be considered valid.
    fwd_lut.apply(&mut buffer_image).unwrap();

    let error_threshold = 1e-6f32;
    for i in 0..12 {
        check_close(out_image1[i], buffer_image[i], error_threshold);
    }

    //
    // Step 2: Repeat with inversion quality FAST, apply inverse LUT.
    //

    buffer_image = out_image1;

    let inv_lut_data_fast = make_fast_lut3d_from_inverse(&inv_lut_data).unwrap();
    let mut inv_lut_fast = lut3d_op(inv_lut_data_fast);
    inv_lut_fast.validate().unwrap();
    inv_lut_fast.finalize().unwrap();
    inv_lut_fast.apply(&mut buffer_image).unwrap();

    fwd_lut.apply(&mut buffer_image).unwrap();

    // Note that, even more than with Lut1D, the FAST inv Lut3D renderer is not exact. It is
    // expected that a fairly loose tolerance must be used here.
    let error_loose = 0.015f32;
    for i in 0..12 {
        check_close(out_image1[i], buffer_image[i], error_loose);
    }

    //
    // Step 3: Test clamping of large values in EXACT mode.
    //
    // Note: No need to test FAST mode since the forward LUT eval clamps inputs to the input
    //       domain.
    //

    buffer_image[0] = 100.0;

    inv_lut.validate().unwrap();
    inv_lut.finalize().unwrap();
    // `apply(bufferImage, 1)`: the first pixel.
    inv_lut.apply(&mut buffer_image[..4]).unwrap();

    // This tests that extreme large values get inverted. (If no inverse is found, apply()
    // currently returns zeros.)
    assert!(buffer_image[0] > 0.5f32);
}

/// Port of `OCIO_ADD_TEST(Lut3DOp, cpu_renderer_lut3d_with_nan)` @ v2.5.2.
#[test]
fn cpu_renderer_lut3d_with_nan() {
    let mut ops = build("clf/lut3d_identity_12i_16f.clf");

    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::NONE).unwrap();
    assert_eq!(1, ops.len());
    let op0 = &ops[0];
    assert_eq!(op0.data().get_type(), OpDataType::Lut3D);

    let qnan = f32::NAN;
    let mut my_image = [
        qnan, 0.25, 0.25, 0.0, //
        0.25, qnan, 0.25, 0.0, //
        0.25, 0.25, qnan, 0.0, //
        0.25, 0.25, 0.0, qnan, //
        0.5, 0.5, 0.5, 0.0,
    ];

    op0.apply(&mut my_image).unwrap();

    assert_eq!(my_image[0], 0.0f32);
    assert_eq!(my_image[1], 0.25f32);
    assert_eq!(my_image[2], 0.25f32);
    assert_eq!(my_image[3], 0.0f32);

    assert_eq!(my_image[5], 0.0f32);
    assert_eq!(my_image[10], 0.0f32);

    assert!(my_image[15].is_nan());

    assert_eq!(my_image[16], 0.5f32);
    assert_eq!(my_image[17], 0.5f32);
    assert_eq!(my_image[18], 0.5f32);
    assert_eq!(my_image[19], 0.0f32);
}
